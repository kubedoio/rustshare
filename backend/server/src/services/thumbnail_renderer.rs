use async_trait::async_trait;
use rustshare_core::{
    domain::{ThumbnailCategory, ThumbnailSize},
    services::{RenderedThumbnail, ThumbnailRenderer},
};
use std::sync::Arc;

#[derive(Default)]
pub struct ImageThumbnailRenderer;

pub fn default_thumbnail_renderer() -> Arc<dyn ThumbnailRenderer> {
    Arc::new(ImageThumbnailRenderer)
}

#[async_trait]
impl ThumbnailRenderer for ImageThumbnailRenderer {
    async fn render(
        &self,
        content: &[u8],
        file_name: &str,
        size: ThumbnailSize,
        category: ThumbnailCategory,
    ) -> Result<RenderedThumbnail, String> {
        match category {
            ThumbnailCategory::Image => render_image(content, size).await,
            ThumbnailCategory::Pdf => render_pdf(size).await,
            ThumbnailCategory::Video => render_video(size).await,
            ThumbnailCategory::Diagram => render_diagram(file_name, size).await,
            ThumbnailCategory::Unsupported => {
                Err("Unsupported file type for thumbnail generation".to_string())
            }
        }
    }
}

async fn render_image(content: &[u8], size: ThumbnailSize) -> Result<RenderedThumbnail, String> {
    let (width, height) = size.dimensions();
    let content_len = content.len();
    let content = content.to_vec();
    let result = tokio::task::spawn_blocking(move || {
        use image::imageops::FilterType;

        tracing::debug!(
            content_len,
            width,
            height,
            "Starting image thumbnail generation"
        );
        let img = image::load_from_memory(&content).map_err(|e| {
            tracing::error!(error = %e, "Failed to load image from memory");
            format!("Failed to load image: {e}")
        })?;
        tracing::debug!(
            original_width = img.width(),
            original_height = img.height(),
            "Image loaded successfully"
        );

        let resized = img.resize(width, height, FilterType::Lanczos3);
        tracing::debug!(
            resized_width = resized.width(),
            resized_height = resized.height(),
            "Image resized"
        );
        let rgba = resized.to_rgba8();
        tracing::debug!(rgba_len = rgba.len(), "Encoding WebP");
        let output = encode_webp(&rgba)?;
        tracing::debug!(output_len = output.len(), "WebP encoding successful");
        Ok(RenderedThumbnail {
            bytes: output,
            content_type: "image/webp".to_string(),
        })
    })
    .await;

    match result {
        Ok(result) => result,
        Err(join_err) => {
            tracing::error!(error = %join_err, "Thumbnail generation task panicked or was cancelled");
            Err(format!("Task failed: {join_err}"))
        }
    }
}

async fn render_pdf(size: ThumbnailSize) -> Result<RenderedThumbnail, String> {
    let (width, height) = size.dimensions();
    let result = tokio::task::spawn_blocking(move || {
        // Keep the current placeholder until PDF page rendering is implemented.
        let mut img = image::RgbaImage::new(width, height);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([220, 53, 69, 255]);
        }
        encode_rendered_webp(&img)
    })
    .await
    .map_err(|e| format!("PDF thumbnail task failed: {e}"))?;
    result
}

async fn render_video(size: ThumbnailSize) -> Result<RenderedThumbnail, String> {
    let (width, height) = size.dimensions();
    let result = tokio::task::spawn_blocking(move || {
        // Keep the current play-button placeholder until frame extraction exists.
        let mut img = image::RgbaImage::new(width, height);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([30, 30, 30, 255]);
        }

        let play_color = image::Rgba([255, 255, 255, 200]);
        let center_x = width / 2;
        let center_y = height / 2;
        let triangle_size = width.min(height) / 4;
        for y in
            center_y.saturating_sub(triangle_size / 2)..=center_y.saturating_add(triangle_size / 2)
        {
            for x in center_x..=center_x.saturating_add(triangle_size) {
                if x < width && y < height {
                    let dy = y as i32 - center_y as i32;
                    let dx = x as i32 - center_x as i32;
                    if dx.abs() <= triangle_size as i32 && dy.abs() <= dx.abs() {
                        img.put_pixel(x, y, play_color);
                    }
                }
            }
        }
        encode_rendered_webp(&img)
    })
    .await
    .map_err(|e| format!("Video thumbnail task failed: {e}"))?;
    result
}

async fn render_diagram(file_name: &str, size: ThumbnailSize) -> Result<RenderedThumbnail, String> {
    let (width, height) = size.dimensions();
    let file_name = file_name.to_lowercase();
    let result = tokio::task::spawn_blocking(move || {
        let mut img = image::RgbaImage::new(width, height);
        let (bg_color, icon_color) =
            if file_name.ends_with(".excalidraw") || file_name.ends_with(".excalidraw.json") {
                (
                    image::Rgba([255, 250, 240, 255]),
                    image::Rgba([105, 67, 53, 255]),
                )
            } else if file_name.ends_with(".drawio") || file_name.ends_with(".dio") {
                (
                    image::Rgba([240, 248, 255, 255]),
                    image::Rgba([46, 125, 247, 255]),
                )
            } else {
                (
                    image::Rgba([245, 245, 245, 255]),
                    image::Rgba([100, 100, 100, 255]),
                )
            };
        for pixel in img.pixels_mut() {
            *pixel = bg_color;
        }
        for x in 0..width {
            img.put_pixel(x, 0, icon_color);
            img.put_pixel(x, height - 1, icon_color);
        }
        for y in 0..height {
            img.put_pixel(0, y, icon_color);
            img.put_pixel(width - 1, y, icon_color);
        }
        encode_rendered_webp(&img)
    })
    .await
    .map_err(|e| format!("Diagram thumbnail task failed: {e}"))?;
    result
}

fn encode_webp(image: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut output)
        .encode(
            image,
            image.width(),
            image.height(),
            image::ColorType::Rgba8.into(),
        )
        .map_err(|e| {
            tracing::error!(error = %e, "WebP encoding failed");
            format!("WebP encode failed: {e}")
        })?;
    Ok(output)
}

fn encode_rendered_webp(image: &image::RgbaImage) -> Result<RenderedThumbnail, String> {
    Ok(RenderedThumbnail {
        bytes: encode_webp(image)?,
        content_type: "image/webp".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat};

    #[tokio::test]
    async fn default_image_renderer_resizes_to_requested_size_and_returns_webp() {
        let mut input = std::io::Cursor::new(Vec::new());
        DynamicImage::new_rgb8(100, 50)
            .write_to(&mut input, ImageFormat::Png)
            .unwrap();

        let rendered = ImageThumbnailRenderer
            .render(
                input.get_ref(),
                "photo.png",
                ThumbnailSize::Sm,
                ThumbnailCategory::Image,
            )
            .await
            .unwrap();
        assert_eq!(rendered.content_type, "image/webp");
        let decoded = image::load_from_memory(&rendered.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (40, 20));
    }

    #[tokio::test]
    async fn default_image_renderer_preserves_invalid_image_error() {
        let error = ImageThumbnailRenderer
            .render(
                b"not an image",
                "photo.png",
                ThumbnailSize::Sm,
                ThumbnailCategory::Image,
            )
            .await
            .unwrap_err();
        assert!(error.starts_with("Failed to load image:"));
    }
}
