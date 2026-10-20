//! 回复图片校验：按文件签名判断真实类型，执行限额。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use turndock_core::limits;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidImage {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("最多 {} 张图片", limits::IMAGES)]
    TooMany,
    #[error("第 {0} 张图片不是有效的 base64")]
    BadEncoding(usize),
    #[error("第 {0} 张图片超过 {max} MiB", max = limits::IMAGE_BYTES / 1024 / 1024)]
    TooLarge(usize),
    #[error("图片合计超过 {} MiB", limits::IMAGES_TOTAL_BYTES / 1024 / 1024)]
    TotalTooLarge,
    #[error("第 {0} 张图片不是 PNG / JPEG / WebP / GIF")]
    Unsupported(usize),
}

pub fn validate(images: &[String]) -> Result<Vec<ValidImage>, ImageError> {
    if images.len() > limits::IMAGES {
        return Err(ImageError::TooMany);
    }
    let mut total = 0usize;
    let mut out = Vec::with_capacity(images.len());
    for (i, data) in images.iter().enumerate() {
        let n = i + 1;
        // 粗略预检，避免解码超大输入。
        if data.len() / 4 * 3 > limits::IMAGE_BYTES + 3 {
            return Err(ImageError::TooLarge(n));
        }
        let bytes = STANDARD
            .decode(data.trim())
            .map_err(|_| ImageError::BadEncoding(n))?;
        if bytes.len() > limits::IMAGE_BYTES {
            return Err(ImageError::TooLarge(n));
        }
        total += bytes.len();
        if total > limits::IMAGES_TOTAL_BYTES {
            return Err(ImageError::TotalTooLarge);
        }
        let mime = sniff(&bytes).ok_or(ImageError::Unsupported(n))?;
        out.push(ValidImage { mime, bytes });
    }
    Ok(out)
}

fn sniff(bytes: &[u8]) -> Option<&'static str> {
    match infer::get(bytes)?.mime_type() {
        "image/png" => Some("image/png"),
        "image/jpeg" => Some("image/jpeg"),
        "image/webp" => Some("image/webp"),
        "image/gif" => Some("image/gif"),
        _ => None,
    }
}

pub fn encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D', b'R',
    ];
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0];

    #[test]
    fn mime_comes_from_bytes_not_claims() {
        let out = validate(&[encode(JPEG), encode(PNG)]).expect("valid");
        assert_eq!(out[0].mime, "image/jpeg");
        assert_eq!(out[1].mime, "image/png");
    }

    #[test]
    fn rejects_svg_and_garbage() {
        let svg = encode(b"<svg xmlns='http://www.w3.org/2000/svg'><script/></svg>");
        assert_eq!(validate(&[svg]), Err(ImageError::Unsupported(1)));
        assert_eq!(
            validate(&["not base64!".into()]),
            Err(ImageError::BadEncoding(1))
        );
    }

    #[test]
    fn enforces_count() {
        let five = vec![encode(PNG); 5];
        assert_eq!(validate(&five), Err(ImageError::TooMany));
    }
}
