//! 输入限额。见 docs/04-协议与状态.md 第 7 节。

pub const MESSAGE_BYTES: usize = 256 * 1024;
pub const TITLE_CHARS: usize = 160;
pub const OPTIONS: usize = 32;
pub const OPTION_CHARS: usize = 500;
pub const REPLY_TEXT_BYTES: usize = 64 * 1024;
pub const IMAGES: usize = 4;
pub const IMAGE_BYTES: usize = 10 * 1024 * 1024;
pub const IMAGES_TOTAL_BYTES: usize = 20 * 1024 * 1024;
/// IPC 单帧上限：图片 base64 后约 27MiB，留出余量。
pub const FRAME_BYTES: usize = 32 * 1024 * 1024;
/// 每个连接同时进行中的请求。
pub const PER_CONNECTION_OPEN: usize = 16;
/// 全局未结束请求。
pub const GLOBAL_OPEN: usize = 256;
/// 请求默认截止时间。
pub const DEFAULT_DEADLINE_MS: i64 = 24 * 60 * 60 * 1000;
