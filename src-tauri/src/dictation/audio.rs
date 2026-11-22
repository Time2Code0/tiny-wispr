//! The audio thread: owns the [`AudioSource`] so opening a device never
//! blocks the runtime, and reports back over the runtime's channel.

use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

use tw_core::dictation::machine::{Event, JobId};

use super::{AudioSource, Msg};

enum Command {
    Start { job: JobId, device: Option<String> },
    Finish { job: JobId },
    Cancel { job: JobId, keep: bool },
    Monitor(Option<MicTestStart>),
}

/// Starts the microphone test on `device` and reports whether it opened.
pub struct MicTestStart {
    pub device: Option<String>,
    pub reply: Sender<tw_audio::Result<()>>,
}

pub struct AudioThread {
    commands: Option<Sender<Command>>,
    thread: Option<JoinHandle<()>>,
}

impl AudioThread {
    pub fn spawn(mut source: Box<dyn AudioSource>, out: Sender<Msg>) -> Self {
        let (commands, rx) = channel::<Command>();
        let thread = std::thread::Builder::new()
            .name("tw-audio".into())
            .spawn(move || {
                for command in rx {
                    let msg = match command {
                        Command::Start { job, device } => {
                            source.stop_monitor();
                            Some(Msg::Event(match source.start(device.as_deref()) {
                                Ok(()) => Event::CaptureStarted { job },
                                Err(problem) => Event::CaptureFailed { job, problem },
                            }))
                        }
                        Command::Finish { job } | Command::Cancel { job, keep: true } => {
                            Some(Msg::Captured {
                                job,
                                recording: source.stop(),
                            })
                        }
                        Command::Cancel { keep: false, .. } => {
                            source.cancel();
                            None
                        }
                        Command::Monitor(Some(MicTestStart { device, reply })) => {
                            let result = source.start_monitor(device.as_deref());
                            if let Err(err) = &result {
                                tracing::warn!(%err, "microphone test could not start");
                            }
                            let _ = reply.send(result);
                            None
                        }
                        Command::Monitor(None) => {
                            source.stop_monitor();
                            None
                        }
                    };
                    if let Some(msg) = msg
                        && out.send(msg).is_err()
                    {
                        break;
                    }
                }
                source.cancel();
            });
        match thread {
            Ok(thread) => Self {
                commands: Some(commands),
                thread: Some(thread),
            },
            Err(err) => {
                tracing::error!(%err, "could not start the audio thread");
                Self {
                    commands: None,
                    thread: None,
                }
            }
        }
    }

    fn send(&self, command: Command) {
        if let Some(commands) = &self.commands
            && commands.send(command).is_err()
        {
            tracing::error!("the audio thread is gone");
        }
    }

    pub fn start(&self, job: JobId, device: Option<String>) {
        self.send(Command::Start { job, device });
    }

    pub fn finish(&self, job: JobId) {
        self.send(Command::Finish { job });
    }

    pub fn cancel(&self, job: JobId, keep: bool) {
        self.send(Command::Cancel { job, keep });
    }

    /// `Some` starts the microphone test, `None` stops it.
    pub fn monitor(&self, start: Option<MicTestStart>) {
        self.send(Command::Monitor(start));
    }
}

impl Drop for AudioThread {
    fn drop(&mut self) {
        self.commands = None;
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("the audio thread panicked");
        }
    }
}
