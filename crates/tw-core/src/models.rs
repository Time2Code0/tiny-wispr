//! Model catalog types, download status and hardware-based recommendation.

use serde::{Deserialize, Serialize};

use crate::hotkey::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelKind {
    /// Whisper (`ggml-*.bin`), stored in `<app data>/models/whisper`.
    Voice,
    /// GGUF language model, stored in `<app data>/models/llm`.
    Language,
}

impl ModelKind {
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Voice => "whisper",
            Self::Language => "llm",
        }
    }
}

/// How far a model file was compressed (quantized) from the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Compression {
    /// The original file, or a language model's only file.
    #[default]
    None,
    /// 8-bit: close to the original, about half the size.
    Light,
    /// 5-bit: about a third of the size, sometimes a little less accurate.
    Strong,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ModelStatus {
    #[default]
    NotDownloaded,
    Downloading {
        received: u64,
        total: u64,
    },
    Paused {
        received: u64,
        total: u64,
    },
    Downloaded,
    Failed {
        message: String,
    },
}

/// One voice or language model, from the catalog or imported by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub kind: ModelKind,
    pub label: String,
    /// One plain sentence for people who don't know the model.
    #[serde(default)]
    pub description: String,
    /// `"whisper"`, `"qwen"`, `"llama"`, `"mistral"`, `"deepseek"`, or `"custom"`.
    pub family: String,
    pub file: String,
    pub url: Option<String>,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    /// The id of the model this is a compressed copy of, so the UI can show
    /// the copies together.
    #[serde(default)]
    pub variant_of: Option<String>,
    #[serde(default)]
    pub compression: Compression,
    /// Voice models only.
    pub english_only: bool,
    /// 1 (slow) to 5 (fast); 0 when unknown (imported models).
    pub speed: u8,
    /// 1 (rough) to 5 (accurate); 0 when unknown (imported models).
    pub accuracy: u8,
    pub ram_mb: u32,
    /// Language models only: the longest context the model was trained for.
    pub context_length: Option<u32>,
    /// Emits `<think>` blocks that must be stripped, and needs a larger token budget.
    pub reasoning: bool,
    pub recommended: bool,
    /// Imported by the user rather than from the catalog.
    pub custom: bool,
    pub status: ModelStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadState {
    Downloading,
    Paused,
    Done,
    Failed,
    Cancelled,
}

/// Payload of `models:progress`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgress {
    pub id: String,
    pub received: u64,
    pub total: u64,
    pub state: DownloadState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub platform: Platform,
    pub arch: String,
    /// 0 when it couldn't be read.
    pub total_memory_bytes: u64,
    pub cpu_cores: u32,
    /// Also means whisper.cpp and llama.cpp run on the GPU through Metal.
    pub apple_silicon: bool,
}

/// One recommended model and why, in words the UI can show as is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub voice: Option<Pick>,
    pub language: Option<Pick>,
}

impl Recommendation {
    /// Sets `recommended` on exactly the picked models.
    pub fn mark(&self, models: &mut [ModelInfo]) {
        for model in models {
            let pick = match model.kind {
                ModelKind::Voice => &self.voice,
                ModelKind::Language => &self.language,
            };
            model.recommended = pick.as_ref().is_some_and(|p| p.id == model.id);
        }
    }
}

// ── catalog ──────────────────────────────────────────────────────────────
//
// Every file is pinned to a repository commit, so the size and sha256 below
// can't drift from what gets downloaded. Sizes and hashes come from the
// Hugging Face API (`X-Linked-Size`, `X-Linked-Etag`); the network test in
// `tw-models` HEADs every URL and compares them.

const HUGGING_FACE: &str = "https://huggingface.co";

struct Entry {
    id: &'static str,
    kind: ModelKind,
    family: &'static str,
    label: &'static str,
    description: &'static str,
    repo: &'static str,
    revision: &'static str,
    file: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    variant_of: Option<&'static str>,
    compression: Compression,
    english_only: bool,
    speed: u8,
    accuracy: u8,
    ram_mb: u32,
    context_length: Option<u32>,
    reasoning: bool,
}

const WHISPER: Entry = Entry {
    id: "",
    kind: ModelKind::Voice,
    family: "whisper",
    label: "",
    description: "",
    repo: "ggerganov/whisper.cpp",
    revision: "5359861c739e955e79d9a303bcbc70fb988958b1",
    file: "",
    size_bytes: 0,
    sha256: "",
    variant_of: None,
    compression: Compression::None,
    english_only: false,
    speed: 0,
    accuracy: 0,
    ram_mb: 0,
    context_length: None,
    reasoning: false,
};

const GGUF: Entry = Entry {
    kind: ModelKind::Language,
    family: "",
    repo: "",
    revision: "",
    ..WHISPER
};

/// Speed and accuracy are relative within a kind. RAM is what the model needs
/// while running (weights plus working buffers), rounded up.
const CATALOG: &[Entry] = &[
    // ── voice: OpenAI Whisper in whisper.cpp's GGML format (MIT) ──
    Entry {
        id: "tiny",
        label: "Tiny",
        description: "Smallest and fastest. Rough with accents and background noise.",
        file: "ggml-tiny.bin",
        size_bytes: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        speed: 5,
        accuracy: 1,
        ram_mb: 280,
        ..WHISPER
    },
    Entry {
        id: "tiny-q5_1",
        label: "Tiny (compressed)",
        description: "The smallest download. Rough with accents and background noise.",
        file: "ggml-tiny-q5_1.bin",
        size_bytes: 32_152_673,
        sha256: "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7",
        speed: 5,
        accuracy: 1,
        ram_mb: 200,
        variant_of: Some("tiny"),
        compression: Compression::Strong,
        ..WHISPER
    },
    Entry {
        id: "tiny.en",
        label: "Tiny (English)",
        description: "Fastest. Fine for short, clear English.",
        file: "ggml-tiny.en.bin",
        size_bytes: 77_704_715,
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        english_only: true,
        speed: 5,
        accuracy: 1,
        ram_mb: 280,
        ..WHISPER
    },
    Entry {
        id: "tiny.en-q5_1",
        label: "Tiny (English, compressed)",
        description: "The smallest download. Fine for short, clear English.",
        file: "ggml-tiny.en-q5_1.bin",
        size_bytes: 32_166_155,
        sha256: "c77c5766f1cef09b6b7d47f21b546cbddd4157886b3b5d6d4f709e91e66c7c2b",
        english_only: true,
        speed: 5,
        accuracy: 1,
        ram_mb: 200,
        variant_of: Some("tiny.en"),
        compression: Compression::Strong,
        ..WHISPER
    },
    Entry {
        id: "base",
        label: "Base",
        description: "Fast and small. Good for clear speech in most languages.",
        file: "ggml-base.bin",
        size_bytes: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        speed: 5,
        accuracy: 2,
        ram_mb: 390,
        ..WHISPER
    },
    Entry {
        id: "base-q5_1",
        label: "Base (compressed)",
        description: "A smaller download of Base with nearly the same accuracy.",
        file: "ggml-base-q5_1.bin",
        size_bytes: 59_707_625,
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        speed: 5,
        accuracy: 2,
        ram_mb: 300,
        variant_of: Some("base"),
        compression: Compression::Strong,
        ..WHISPER
    },
    Entry {
        id: "base.en",
        label: "Base (English)",
        description: "Fast and small. Good for clear English.",
        file: "ggml-base.en.bin",
        size_bytes: 147_964_211,
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
        english_only: true,
        speed: 5,
        accuracy: 2,
        ram_mb: 390,
        ..WHISPER
    },
    Entry {
        id: "base.en-q5_1",
        label: "Base (English, compressed)",
        description: "A smaller download of Base (English) with nearly the same accuracy.",
        file: "ggml-base.en-q5_1.bin",
        size_bytes: 59_721_011,
        sha256: "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f",
        english_only: true,
        speed: 5,
        accuracy: 2,
        ram_mb: 300,
        variant_of: Some("base.en"),
        compression: Compression::Strong,
        ..WHISPER
    },
    Entry {
        id: "small",
        label: "Small",
        description: "A good balance of speed and accuracy in many languages.",
        file: "ggml-small.bin",
        size_bytes: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        speed: 4,
        accuracy: 3,
        ram_mb: 860,
        ..WHISPER
    },
    Entry {
        id: "small-q5_1",
        label: "Small (compressed)",
        description: "A smaller download of Small with nearly the same accuracy.",
        file: "ggml-small-q5_1.bin",
        size_bytes: 190_085_487,
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        speed: 4,
        accuracy: 3,
        ram_mb: 560,
        variant_of: Some("small"),
        compression: Compression::Strong,
        ..WHISPER
    },
    Entry {
        id: "small.en",
        label: "Small (English)",
        description: "A good balance of speed and accuracy for English.",
        file: "ggml-small.en.bin",
        size_bytes: 487_614_201,
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
        english_only: true,
        speed: 4,
        accuracy: 3,
        ram_mb: 860,
        ..WHISPER
    },
    Entry {
        id: "small.en-q5_1",
        label: "Small (English, compressed)",
        description: "A smaller download of Small (English) with nearly the same accuracy.",
        file: "ggml-small.en-q5_1.bin",
        size_bytes: 190_098_681,
        sha256: "bfdff4894dcb76bbf647d56263ea2a96645423f1669176f4844a1bf8e478ad30",
        english_only: true,
        speed: 4,
        accuracy: 3,
        ram_mb: 560,
        variant_of: Some("small.en"),
        compression: Compression::Strong,
        ..WHISPER
    },
    Entry {
        id: "medium",
        label: "Medium",
        description: "Accurate in many languages, but slow without a fast GPU.",
        file: "ggml-medium.bin",
        size_bytes: 1_533_763_059,
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        speed: 2,
        accuracy: 4,
        ram_mb: 2_100,
        ..WHISPER
    },
    Entry {
        id: "medium-q5_0",
        label: "Medium (compressed)",
        description: "A smaller download of Medium with nearly the same accuracy.",
        file: "ggml-medium-q5_0.bin",
        size_bytes: 539_212_467,
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        speed: 2,
        accuracy: 4,
        ram_mb: 1_100,
        variant_of: Some("medium"),
        compression: Compression::Strong,
        ..WHISPER
    },
    Entry {
        id: "medium.en",
        label: "Medium (English)",
        description: "Accurate English, but slow without a fast GPU.",
        file: "ggml-medium.en.bin",
        size_bytes: 1_533_774_781,
        sha256: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
        english_only: true,
        speed: 2,
        accuracy: 4,
        ram_mb: 2_100,
        ..WHISPER
    },
    Entry {
        id: "medium.en-q5_0",
        label: "Medium (English, compressed)",
        description: "A smaller download of Medium (English) with nearly the same accuracy.",
        file: "ggml-medium.en-q5_0.bin",
        size_bytes: 539_225_533,
        sha256: "76733e26ad8fe1c7a5bf7531a9d41917b2adc0f20f2e4f5531688a8c6cd88eb0",
        english_only: true,
        speed: 2,
        accuracy: 4,
        ram_mb: 1_100,
        variant_of: Some("medium.en"),
        compression: Compression::Strong,
        ..WHISPER
    },
    Entry {
        id: "large-v3-turbo",
        label: "Large v3 Turbo",
        description: "Close to the most accurate, and much faster than Large v3.",
        file: "ggml-large-v3-turbo.bin",
        size_bytes: 1_624_555_275,
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        speed: 3,
        accuracy: 5,
        ram_mb: 2_000,
        ..WHISPER
    },
    Entry {
        id: "large-v3-turbo-q8_0",
        label: "Large v3 Turbo (lightly compressed)",
        description: "Half the download of Large v3 Turbo with the same accuracy in practice.",
        file: "ggml-large-v3-turbo-q8_0.bin",
        size_bytes: 874_188_075,
        sha256: "317eb69c11673c9de1e1f0d459b253999804ec71ac4c23c17ecf5fbe24e259a1",
        speed: 3,
        accuracy: 5,
        ram_mb: 1_250,
        variant_of: Some("large-v3-turbo"),
        compression: Compression::Light,
        ..WHISPER
    },
    Entry {
        id: "large-v3-turbo-q5_0",
        label: "Large v3 Turbo (compressed)",
        description: "Accurate in many languages at a third of the download.",
        file: "ggml-large-v3-turbo-q5_0.bin",
        size_bytes: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        speed: 3,
        accuracy: 4,
        ram_mb: 950,
        variant_of: Some("large-v3-turbo"),
        compression: Compression::Strong,
        ..WHISPER
    },
    Entry {
        id: "large-v3",
        label: "Large v3",
        description: "The most accurate. Slow, and needs a powerful computer.",
        file: "ggml-large-v3.bin",
        size_bytes: 3_095_033_483,
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        speed: 1,
        accuracy: 5,
        ram_mb: 3_900,
        ..WHISPER
    },
    Entry {
        id: "large-v3-q5_0",
        label: "Large v3 (compressed)",
        description: "A smaller download of Large v3 with nearly the same accuracy. Slow.",
        file: "ggml-large-v3-q5_0.bin",
        size_bytes: 1_081_140_203,
        sha256: "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
        speed: 1,
        accuracy: 5,
        ram_mb: 1_900,
        variant_of: Some("large-v3"),
        compression: Compression::Strong,
        ..WHISPER
    },
    // ── language: GGUF, Q4_K_M ──
    // Llama 3.2 Community License. Template: llama3.
    Entry {
        id: "llama-3.2-1b-q4_k_m",
        family: "llama",
        label: "Llama 3.2 1B",
        description: "The fastest cleanup. Fixes fillers and punctuation, may miss subtler mistakes.",
        repo: "lmstudio-community/Llama-3.2-1B-Instruct-GGUF",
        revision: "199151125cf15a129ab3b548b26afeed976df066",
        file: "Llama-3.2-1B-Instruct-Q4_K_M.gguf",
        size_bytes: 807_690_688,
        sha256: "f7ede42862ceca07ad1c88a97b67520019c4ac7e5ced250d2e696fa62ab189af",
        speed: 5,
        accuracy: 2,
        ram_mb: 1_300,
        context_length: Some(131_072),
        ..GGUF
    },
    Entry {
        id: "llama-3.2-3b-q4_k_m",
        family: "llama",
        label: "Llama 3.2 3B",
        description: "Reliable cleanup for English at a moderate size.",
        repo: "lmstudio-community/Llama-3.2-3B-Instruct-GGUF",
        revision: "c91307b5cf18c8106b1f8a6218c26ae4dbfee472",
        file: "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        size_bytes: 2_019_377_440,
        sha256: "e4f1a04d927b09ec18eb2f233d85ecd760fc2d35cec97e37f8604d3632210d9a",
        speed: 3,
        accuracy: 3,
        ram_mb: 2_700,
        context_length: Some(131_072),
        ..GGUF
    },
    // Apache 2.0. Template: ChatML. Qwen3.5 can think but is off by default at
    // 2B: its template opens the answer with an empty `<think>\n\n</think>\n\n`,
    // which a generic ChatML template must add itself. Needs llama.cpp with the
    // `qwen35` architecture (February 2026 or later).
    Entry {
        id: "qwen3.5-2b-q4_k_m",
        family: "qwen",
        label: "Qwen 3.5 2B",
        description: "Fast, small and good in many languages.",
        repo: "unsloth/Qwen3.5-2B-GGUF",
        revision: "f6d5376be1edb4d416d56da11e5397a961aca8ae",
        file: "Qwen3.5-2B-Q4_K_M.gguf",
        size_bytes: 1_280_835_840,
        sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
        speed: 4,
        accuracy: 3,
        ram_mb: 1_900,
        context_length: Some(262_144),
        ..GGUF
    },
    // Apache 2.0. Template: ChatML. The 2507 instruct release never thinks.
    Entry {
        id: "qwen3-4b-instruct-2507-q4_k_m",
        family: "qwen",
        label: "Qwen 3 4B Instruct",
        description: "Careful, accurate cleanup in many languages. A good default.",
        repo: "unsloth/Qwen3-4B-Instruct-2507-GGUF",
        revision: "a06e946bb6b655725eafa393f4a9745d460374c9",
        file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        size_bytes: 2_497_281_120,
        sha256: "3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597",
        speed: 3,
        accuracy: 4,
        ram_mb: 3_300,
        context_length: Some(262_144),
        ..GGUF
    },
    // Apache 2.0, Mistral's own GGUF. Template: Mistral v7 (tekken) with a
    // system prompt. Needs llama.cpp with the `mistral3` architecture
    // (December 2025 or later).
    Entry {
        id: "ministral-3-3b-q4_k_m",
        family: "mistral",
        label: "Ministral 3 3B",
        description: "Balanced cleanup, strong in European languages.",
        repo: "mistralai/Ministral-3-3B-Instruct-2512-GGUF",
        revision: "eb599d408350ea2bb60452cb86be7c7b2fc28227",
        file: "Ministral-3-3B-Instruct-2512-Q4_K_M.gguf",
        size_bytes: 2_147_023_008,
        sha256: "9ed150d4367e68df0ac8e1540f6ddc65b42d0ee26378329d1ecbca60f93fc5f8",
        speed: 3,
        accuracy: 4,
        ram_mb: 2_900,
        context_length: Some(262_144),
        ..GGUF
    },
    Entry {
        id: "ministral-3-8b-q4_k_m",
        family: "mistral",
        label: "Ministral 3 8B",
        description: "The most polished cleanup. Needs 16 GB of memory or more.",
        repo: "mistralai/Ministral-3-8B-Instruct-2512-GGUF",
        revision: "0102285ad796bd99af90f58de616092e5630e970",
        file: "Ministral-3-8B-Instruct-2512-Q4_K_M.gguf",
        size_bytes: 5_198_911_904,
        sha256: "33e7a72cf5e6e2cfc2f2847075acc013d68bba023e35310cef86b5cf8fdca761",
        speed: 2,
        accuracy: 5,
        ram_mb: 6_300,
        context_length: Some(262_144),
        ..GGUF
    },
    // MIT. Reasoning models: they always open with `<think>` and that can't be
    // switched off, so strip it and budget a few hundred extra tokens.
    // Template: DeepSeek R1 (`<｜User｜>` / `<｜Assistant｜>`), no system role.
    Entry {
        id: "deepseek-r1-distill-qwen-1.5b-q4_k_m",
        family: "deepseek",
        label: "DeepSeek R1 Distill 1.5B",
        description: "Small, but it often answers or rewrites instead of cleaning up. Not a good first choice.",
        repo: "unsloth/DeepSeek-R1-Distill-Qwen-1.5B-GGUF",
        revision: "3cb4d15544a2a5e07439592b9a0965b6445fbd34",
        file: "DeepSeek-R1-Distill-Qwen-1.5B-Q4_K_M.gguf",
        size_bytes: 1_117_321_312,
        sha256: "f3bdf9cf31dee4b57ae4e455a1cb0d01b5c2c1b50d72d3112141c195506c2840",
        speed: 3,
        accuracy: 1,
        ram_mb: 1_700,
        context_length: Some(131_072),
        reasoning: true,
        ..GGUF
    },
    Entry {
        id: "deepseek-r1-0528-qwen3-8b-q4_k_m",
        family: "deepseek",
        label: "DeepSeek R1 0528 8B",
        description: "Thinks before answering. Thorough, but slow for everyday dictation.",
        repo: "unsloth/DeepSeek-R1-0528-Qwen3-8B-GGUF",
        revision: "eb48357c179d34dbf515983f798dfb8752a0f261",
        file: "DeepSeek-R1-0528-Qwen3-8B-Q4_K_M.gguf",
        size_bytes: 5_027_785_216,
        sha256: "a86349a4180c4e6bb43f874c29c404fa2be3f90b15509bd6d86f697dba724ec1",
        speed: 1,
        accuracy: 4,
        ram_mb: 6_100,
        context_length: Some(131_072),
        reasoning: true,
        ..GGUF
    },
];

impl Entry {
    fn to_info(&self) -> ModelInfo {
        ModelInfo {
            id: self.id.to_owned(),
            kind: self.kind,
            label: self.label.to_owned(),
            description: self.description.to_owned(),
            family: self.family.to_owned(),
            file: self.file.to_owned(),
            url: Some(format!(
                "{HUGGING_FACE}/{}/resolve/{}/{}",
                self.repo, self.revision, self.file
            )),
            size_bytes: self.size_bytes,
            sha256: Some(self.sha256.to_owned()),
            variant_of: self.variant_of.map(str::to_owned),
            compression: self.compression,
            english_only: self.english_only,
            speed: self.speed,
            accuracy: self.accuracy,
            ram_mb: self.ram_mb,
            context_length: self.context_length,
            reasoning: self.reasoning,
            recommended: false,
            custom: false,
            status: ModelStatus::NotDownloaded,
        }
    }
}

/// The built-in catalog (spec §6), every model not downloaded and not recommended.
pub fn catalog() -> Vec<ModelInfo> {
    CATALOG.iter().map(Entry::to_info).collect()
}

// ── recommendation ───────────────────────────────────────────────────────

const GIB: u64 = 1 << 30;

/// Picks one voice and one language model for this machine, from the
/// non-custom `models`. Apple silicon counts as having a GPU: whisper.cpp and
/// llama.cpp use Metal there and the CPU elsewhere.
pub fn recommend(hardware: &HardwareInfo, models: &[ModelInfo]) -> Recommendation {
    let gib = hardware.total_memory_bytes.div_ceil(GIB);
    let gpu = hardware.apple_silicon;
    let cores = hardware.cpu_cores;
    let memory = format!("{gib} GB of memory");

    let (voice_ids, voice_reason): (&[&str], String) = match (gib, gpu) {
        (0, _) => (
            &["base", "base-q5_1", "tiny"],
            "Small and quick, so it runs on any computer.".to_owned(),
        ),
        (24.., true) => (
            &[
                "large-v3-turbo",
                "large-v3-turbo-q8_0",
                "large-v3-turbo-q5_0",
            ],
            format!("Close to the most accurate, and fast on Apple silicon with {memory}."),
        ),
        (12.., true) => (
            &["large-v3-turbo-q5_0", "small"],
            format!("Accurate and fast on Apple silicon with {memory}."),
        ),
        (16.., false) if cores >= 12 => (
            &["large-v3-turbo-q5_0", "small"],
            format!("Accurate, and {cores} processor cores keep it quick."),
        ),
        (6.., _) if gpu || cores >= 4 => (
            &["small", "small-q5_1", "base"],
            format!("A good balance of speed and accuracy with {memory}."),
        ),
        _ => (
            &["base", "base-q5_1", "tiny"],
            format!("Small and quick, so it stays responsive with {memory}."),
        ),
    };

    let (language_ids, language_reason): (&[&str], String) = match (gib, gpu) {
        (0, _) => (
            &["llama-3.2-1b-q4_k_m"],
            "The smallest cleanup model, so it runs on any computer.".to_owned(),
        ),
        (24.., true) => (
            &["ministral-3-8b-q4_k_m", "qwen3-4b-instruct-2507-q4_k_m"],
            format!("The most polished cleanup, and it fits comfortably in {memory}."),
        ),
        (12.., true) | (24.., false) => (
            &["qwen3-4b-instruct-2507-q4_k_m", "llama-3.2-3b-q4_k_m"],
            format!("Careful cleanup that fits comfortably in {memory}."),
        ),
        (12.., false) | (6.., true) => (
            &["qwen3.5-2b-q4_k_m", "llama-3.2-1b-q4_k_m"],
            format!("Quick cleanup that leaves room for your other apps in {memory}."),
        ),
        _ => (
            &["llama-3.2-1b-q4_k_m"],
            format!("The smallest cleanup model, so it fits in {memory}."),
        ),
    };

    Recommendation {
        voice: pick(models, ModelKind::Voice, voice_ids, voice_reason),
        language: pick(models, ModelKind::Language, language_ids, language_reason),
    }
}

/// The first of `preferred` that is in `models`, otherwise the catalog model
/// of `kind` that needs the least memory.
fn pick(models: &[ModelInfo], kind: ModelKind, preferred: &[&str], reason: String) -> Option<Pick> {
    let candidates = || models.iter().filter(|m| m.kind == kind && !m.custom);
    let chosen = preferred
        .iter()
        .find_map(|id| candidates().find(|m| m.id == *id))
        .or_else(|| candidates().min_by_key(|m| m.ram_mb))?;
    let reason = if preferred.contains(&chosen.id.as_str()) {
        reason
    } else {
        "The lightest model available.".to_owned()
    };
    Some(Pick {
        id: chosen.id.clone(),
        reason,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn status_is_tagged() {
        let json = serde_json::to_value(ModelStatus::Downloading {
            received: 1,
            total: 2,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "state": "downloading", "received": 1, "total": 2 })
        );
        let back: ModelStatus = serde_json::from_value(json).unwrap();
        assert_eq!(
            back,
            ModelStatus::Downloading {
                received: 1,
                total: 2
            }
        );
        assert_eq!(
            serde_json::to_value(ModelStatus::NotDownloaded).unwrap(),
            serde_json::json!({ "state": "notDownloaded" })
        );
    }

    #[test]
    fn catalog_ids_and_files_are_unique() {
        let models = catalog();
        let ids: HashSet<_> = models.iter().map(|m| &m.id).collect();
        let files: HashSet<_> = models.iter().map(|m| &m.file).collect();
        assert_eq!(ids.len(), models.len());
        assert_eq!(files.len(), models.len());
    }

    #[test]
    fn catalog_entries_are_complete() {
        for m in catalog() {
            let url = m.url.as_deref().unwrap();
            assert!(
                url.starts_with("https://huggingface.co/")
                    && url.ends_with(&format!("/{}", m.file)),
                "{}: {url}",
                m.id
            );
            assert!(
                url.split('/').nth(6).is_some_and(|rev| rev.len() == 40),
                "{}: not pinned to a commit",
                m.id
            );
            let sha = m.sha256.as_deref().unwrap();
            assert!(
                sha.len() == 64
                    && sha
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{}: bad sha256",
                m.id
            );
            assert!(m.size_bytes > 0, "{}", m.id);
            assert!(!m.label.is_empty() && !m.description.is_empty(), "{}", m.id);
            assert!(
                m.description.ends_with('.') && !m.description.contains('!'),
                "{}",
                m.id
            );
            assert!(
                (1..=5).contains(&m.speed) && (1..=5).contains(&m.accuracy),
                "{}",
                m.id
            );
            assert!(
                u64::from(m.ram_mb) * 1024 * 1024 > m.size_bytes,
                "{}: needs at least its own size in memory",
                m.id
            );
            assert!(!m.custom && !m.recommended && m.status == ModelStatus::NotDownloaded);
            match m.kind {
                ModelKind::Voice => {
                    assert_eq!(m.family, "whisper");
                    assert!(m.file.starts_with("ggml-") && m.file.ends_with(".bin"));
                    assert_eq!(m.context_length, None);
                    assert!(!m.reasoning);
                    assert_eq!(m.english_only, m.id.contains(".en"), "{}", m.id);
                }
                ModelKind::Language => {
                    assert!(m.file.ends_with(".gguf"));
                    assert!(m.context_length.is_some_and(|c| c >= 8_192));
                    assert!(!m.english_only);
                }
            }
        }
    }

    #[test]
    fn catalog_covers_every_family_the_user_named() {
        let models = catalog();
        for family in ["qwen", "llama", "mistral", "deepseek"] {
            let sizes: Vec<_> = models
                .iter()
                .filter(|m| m.family == family)
                .map(|m| m.size_bytes)
                .collect();
            assert!(
                sizes.len() >= 2,
                "{family}: want a small and a larger option"
            );
            assert!(
                sizes.iter().all(|s| *s < 6 * GIB),
                "{family}: too big for a laptop"
            );
        }
        assert!(models.iter().any(|m| m.family == "deepseek" && m.reasoning));
        for id in [
            "tiny",
            "tiny.en",
            "base",
            "base.en",
            "small",
            "small.en",
            "medium",
            "medium.en",
            "large-v3-turbo",
            "large-v3-turbo-q5_0",
            "large-v3",
        ] {
            assert!(models.iter().any(|m| m.id == id), "missing {id}");
        }
    }

    #[test]
    fn compressed_copies_point_at_their_original() {
        let models = catalog();
        for m in &models {
            let Some(original) = &m.variant_of else {
                assert_eq!(m.compression, Compression::None, "{}", m.id);
                continue;
            };
            assert_ne!(m.compression, Compression::None, "{}", m.id);
            let original = models.iter().find(|o| &o.id == original).unwrap();
            assert_eq!(original.variant_of, None, "{}", m.id);
            assert_eq!(original.compression, Compression::None, "{}", m.id);
            assert_eq!(original.kind, m.kind, "{}", m.id);
            assert_eq!(original.english_only, m.english_only, "{}", m.id);
            assert!(m.size_bytes < original.size_bytes, "{}", m.id);
        }
    }

    #[test]
    fn catalog_serialises_for_the_ui() {
        let json = serde_json::to_value(&catalog()[0]).unwrap();
        for key in [
            "id",
            "kind",
            "label",
            "description",
            "family",
            "file",
            "url",
            "sizeBytes",
            "sha256",
            "variantOf",
            "compression",
            "englishOnly",
            "speed",
            "accuracy",
            "ramMb",
            "contextLength",
            "reasoning",
            "recommended",
            "custom",
            "status",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
    }

    fn machine(gib: u64, cores: u32, apple_silicon: bool) -> HardwareInfo {
        HardwareInfo {
            platform: if apple_silicon {
                Platform::Macos
            } else {
                Platform::Windows
            },
            arch: if apple_silicon { "aarch64" } else { "x86_64" }.to_owned(),
            total_memory_bytes: gib * GIB,
            cpu_cores: cores,
            apple_silicon,
        }
    }

    fn ids(rec: &Recommendation) -> (&str, &str) {
        (
            rec.voice.as_ref().map_or("", |p| p.id.as_str()),
            rec.language.as_ref().map_or("", |p| p.id.as_str()),
        )
    }

    #[test]
    fn recommends_by_tier() {
        let models = catalog();
        let cases = [
            (
                machine(36, 14, true),
                ("large-v3-turbo", "ministral-3-8b-q4_k_m"),
            ),
            (
                machine(16, 8, true),
                ("large-v3-turbo-q5_0", "qwen3-4b-instruct-2507-q4_k_m"),
            ),
            (machine(8, 8, true), ("small", "qwen3.5-2b-q4_k_m")),
            (
                machine(32, 16, false),
                ("large-v3-turbo-q5_0", "qwen3-4b-instruct-2507-q4_k_m"),
            ),
            (machine(16, 8, false), ("small", "qwen3.5-2b-q4_k_m")),
            (machine(8, 4, false), ("small", "llama-3.2-1b-q4_k_m")),
            (machine(8, 2, false), ("base", "llama-3.2-1b-q4_k_m")),
            (machine(4, 4, false), ("base", "llama-3.2-1b-q4_k_m")),
        ];
        for (hw, want) in cases {
            let rec = recommend(&hw, &models);
            assert_eq!(ids(&rec), want, "{hw:?}");
            let voice = models.iter().find(|m| m.id == want.0).unwrap();
            let language = models.iter().find(|m| m.id == want.1).unwrap();
            assert!(
                u64::from(voice.ram_mb + language.ram_mb) * 1024 * 1024 * 2
                    <= hw.total_memory_bytes,
                "{hw:?}: both picks should fit in half the memory"
            );
        }
    }

    #[test]
    fn reasons_name_the_memory() {
        let rec = recommend(&machine(36, 14, true), &catalog());
        assert_eq!(
            rec.voice.unwrap().reason,
            "Close to the most accurate, and fast on Apple silicon with 36 GB of memory."
        );
        assert_eq!(
            rec.language.unwrap().reason,
            "The most polished cleanup, and it fits comfortably in 36 GB of memory."
        );
    }

    #[test]
    fn unknown_memory_still_recommends_something_small() {
        let rec = recommend(&machine(0, 0, false), &catalog());
        assert_eq!(ids(&rec), ("base", "llama-3.2-1b-q4_k_m"));
        assert!(!rec.voice.unwrap().reason.contains("0 GB"));
    }

    #[test]
    fn falls_back_to_the_lightest_available_model() {
        let models: Vec<_> = catalog()
            .into_iter()
            .filter(|m| ["medium", "tiny.en", "ministral-3-3b-q4_k_m"].contains(&m.id.as_str()))
            .collect();
        let rec = recommend(&machine(36, 14, true), &models);
        assert_eq!(ids(&rec), ("tiny.en", "ministral-3-3b-q4_k_m"));
        assert_eq!(rec.voice.unwrap().reason, "The lightest model available.");
    }

    #[test]
    fn never_recommends_custom_models_or_nothing() {
        let mut custom = catalog()[0].clone();
        custom.id = "custom".to_owned();
        custom.custom = true;
        custom.ram_mb = 1;
        assert_eq!(
            recommend(&machine(16, 8, true), &[custom]),
            Recommendation::default()
        );
    }

    #[test]
    fn mark_flags_only_the_picks() {
        let mut models = catalog();
        models[1].recommended = true;
        let rec = recommend(&machine(16, 8, true), &models);
        rec.mark(&mut models);
        let marked: Vec<_> = models
            .iter()
            .filter(|m| m.recommended)
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(
            marked,
            ["large-v3-turbo-q5_0", "qwen3-4b-instruct-2507-q4_k_m"]
        );
    }
}
