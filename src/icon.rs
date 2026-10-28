use image::{imageops, imageops::FilterType, RgbaImage};

pub fn build_ico(img: &RgbaImage, sizes: &[u32]) -> anyhow::Result<Vec<u8>> {
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    let mut sizes: Vec<u32> = sizes.iter().copied().filter(|s| (1..=256).contains(s)).collect();
    if sizes.is_empty() {
        sizes = vec![16, 32, 48, 256];
    }
    sizes.sort_unstable();
    sizes.dedup();
    for s in sizes {
        let frame = if img.width() == s && img.height() == s {
            img.clone()
        } else {
            imageops::resize(img, s, s, FilterType::Lanczos3)
        };
        let icon = ico::IconImage::from_rgba_data(s, s, frame.into_raw());
        dir.add_entry(ico::IconDirEntry::encode(&icon)?);
    }
    let mut out = Vec::new();
    dir.write(&mut out)?;
    Ok(out)
}
