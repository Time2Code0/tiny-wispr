# Text, styles, dictionary and snippets

Whisper's output goes through a few steps before it's pasted. Everything runs on your computer.

```text
speech → Whisper → spoken punctuation → fillers → (language model) → dictionary → snippets → style → pasted text
```

The first three steps below are about cleanup. The last three are yours to configure.

## Cleanup level

Pick a level on the Style page.

| Level | What it does |
| --- | --- |
| **None** | The raw transcript. Only your dictionary and snippets still apply. |
| **Light** (default) | Turns spoken punctuation into symbols and removes fillers and stutters. With a language model, also fixes grammar, drops false starts, keeps only the correction when you correct yourself, and formats spoken lists. |
| **Medium** | Everything in Light, and with a language model also rewords for clarity and concision. |

Without a language model, Light and Medium behave the same: only the rule-based steps run. The language model is optional, and set up on the [Models](models.md) page.

The model is skipped when it wouldn't help: one- and two-word dictations, short sentences that already read cleanly, and very long dictations (over 500 words, which would take too long). If it fails, or returns text that doesn't look like what you said (new words, a much shorter or longer text, an answer instead of your words), the rule-based text is pasted instead.

### Spoken punctuation

Say the punctuation and it's typed:

| Say | Get |
| --- | --- |
| "period", "full stop" | `.` |
| "comma" | `,` |
| "question mark" | `?` |
| "exclamation point", "exclamation mark" | `!` |
| "colon", "semicolon" | `:` `;` |
| "em dash" | `—` |
| "new line", "next line", "line break" | a line break |
| "new paragraph", "skip a line" | a blank line |

The app tries not to act when you're talking about punctuation rather than dictating it: "remove the comma", "a period of time" and "trial period" stay as words.

### Fillers

"Um", "uh", "hmm" and stuttered words ("the the") are removed. The app is cautious: "uh huh" is an answer and stays, and a word like "that that" stays because it can be correct.

## Styles

Tiny Wispr looks at the app you were in when you finished speaking and picks a writing style for it.

| Category | Apps |
| --- | --- |
| Personal messages | Messages, WhatsApp, Telegram, Signal, Discord and similar |
| Work messages | Slack, Teams, Zoom, Mattermost and similar |
| Email | Mail, Outlook, Spark, Superhuman and similar |
| Other | Everything else: editors, documents, browsers, terminals |

On the Style page, choose a style for each category. Each card shows a sample sentence.

| Style | Result | Available in |
| --- | --- | --- |
| **Formal.** | Full capitals and punctuation. The default everywhere. | All |
| **Casual** | Capitals kept, lighter punctuation. | All |
| **very casual** | No capitals, lighter punctuation. | Personal messages |
| **Excited!** | More exclamation marks. | Work messages, Email, Other |

In the two messaging categories, Casual and very casual drop the final period of a short message. URLs, email addresses, paths and code keep their capitals and punctuation, and so do the words in your dictionary.

## Dictionary

The dictionary has two jobs, on the Dictionary page:

- **Vocabulary.** Add names and jargon ("Kubernetes", "Zoë") so Whisper is nudged to hear them and the language model keeps their spelling.
- **Corrections.** Add a misspelling to replace it with the right word every time ("wispr" → "Wispr").

Matching is whole-word and ignores case. The longest phrase wins, possessives follow ("Bob's"), and your saved capitalisation is used. Words and misspellings can be up to 60 characters. The same word can't be added twice, and a misspelling can't equal its own word.

Each correction counts towards the "fixes made" number on Home.

## Snippets

A snippet is a trigger phrase that expands into longer text, such as "calendar link" → your booking URL. Say the trigger on its own, or inside a sentence. Matching ignores case, and the longest trigger wins.

- A dictation that is only the trigger becomes exactly the expansion, without added punctuation.
- Triggers are up to 60 characters; expansions up to 4,000.
- A trigger can't repeat another one or be the same as a dictionary correction. You get a warning if two triggers are very close, such as one containing the other.

## Try it without dictating

The Style page has a live example: type a sentence and see how your cleanup level, style, dictionary and snippets change it. It only uses the rule-based steps, so it shows what the language model would start from.
