use crate::error::JglcError;
use crate::vfs::Vfs;
use std::io::Write;
use std::path::Path;
use tar::{Builder, Header};
use xz2::write::XzEncoder;

/// Create a .tar.xz bundle from the VFS
pub fn create_bundle(vfs: &Vfs, output_path: &Path) -> Result<(), JglcError> {
    let tar_xz_path = output_path.join("project.tar.xz");
    let file = std::fs::File::create(&tar_xz_path)
        .map_err(|e| JglcError::Io(e))?;

    let encoder = XzEncoder::new(file, 9);
    let mut tar_builder = Builder::new(encoder);

    for path in &vfs.ordered_files {
        if let Some(entry) = vfs.get(path) {
            let data = entry.content.as_bytes();
            
            // Skip empty paths
            let path_str = path.trim();
            if path_str.is_empty() {
                continue;
            }
            
            // Create tar entry header manually
            let mut header = Header::new_ustar();
            header.set_path(path_str).map_err(|e| JglcError::Tar(format!("Path '{}': {}", path_str, e)))?;
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            
            let mut data_slice = data;
            tar_builder.append(&header, &mut data_slice).map_err(|e| JglcError::Tar(e.to_string()))?;
        }
    }

    let w = tar_builder.into_inner().map_err(|e| JglcError::Tar(e.to_string()))?;
    w.finish().map_err(|e| JglcError::Xz(e.to_string()))?;
    
    Ok(())
}

/// Write the manifest block to the bundle as project._jgl
pub fn write_manifest_bundle(
    manifest_content: &str,
    output_dir: &Path,
) -> Result<(), JglcError> {
    let manifest_path = output_dir.join("project._jgl");
    let mut file = std::fs::File::create(&manifest_path)
        .map_err(|e| JglcError::Io(e))?;
    file.write_all(manifest_content.as_bytes()).map_err(|e| JglcError::Io(e))?;
    Ok(())
}
