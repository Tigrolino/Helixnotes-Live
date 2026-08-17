use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};

/// Grants asset-protocol access only to a vault selected by the user.
/// `.helixnotes` is added explicitly because Unix glob matching excludes hidden paths.
pub fn allow_vault_assets<R: Runtime>(app: &AppHandle<R>, vault_path: &Path) -> Result<(), String> {
    // Keep the canonical native path: Tauri's scope normalizes Unix paths plus
    // Windows drive, verbatim, and UNC prefixes when matching asset requests.
    let vault = std::fs::canonicalize(vault_path).map_err(|error| {
        format!(
            "Failed to resolve vault asset path '{}': {error}",
            vault_path.display()
        )
    })?;
    let scope = app.asset_protocol_scope();
    scope
        .allow_directory(&vault, true)
        .map_err(|error| error.to_string())?;
    scope
        .allow_directory(vault.join(".helixnotes"), true)
        .map_err(|error| error.to_string())
}

/// Grants access to relative assets next to a Markdown file explicitly opened by the user.
pub fn allow_external_note_assets<R: Runtime>(
    app: &AppHandle<R>,
    note_path: &Path,
) -> Result<(), String> {
    let note = std::fs::canonicalize(note_path).map_err(|error| {
        format!(
            "Failed to resolve external note path '{}': {error}",
            note_path.display()
        )
    })?;
    let parent = note
        .parent()
        .ok_or_else(|| "External note has no parent directory".to_string())?;
    app.asset_protocol_scope()
        .allow_directory(parent, true)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_directory(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("helixnotes-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn static_asset_scope_does_not_expose_the_filesystem() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let scope = &config["app"]["security"]["assetProtocol"]["scope"];

        assert_eq!(scope["allow"], serde_json::json!([]));
        assert_eq!(scope["deny"], serde_json::json!([]));
        assert_eq!(scope["requireLiteralLeadingDot"], true);
    }

    #[test]
    fn runtime_scope_allows_only_the_selected_vault() {
        let root = test_directory("asset-scope");
        let vault = root.join("Vault [Cross Platform]");
        let attachments = vault.join(".helixnotes").join("attachments");
        fs::create_dir_all(&attachments).unwrap();
        let note_image = vault.join("images").join("note image.png");
        fs::create_dir_all(note_image.parent().unwrap()).unwrap();
        fs::write(&note_image, b"image").unwrap();
        let attachment = attachments.join("attachment.png");
        fs::write(&attachment, b"attachment").unwrap();
        let outside = root.join("outside.png");
        fs::write(&outside, b"outside").unwrap();

        let app = tauri::test::mock_app();
        allow_vault_assets(app.handle(), &vault).unwrap();
        let scope = app.asset_protocol_scope();

        assert!(scope.is_allowed(&note_image));
        assert!(scope.is_allowed(&attachment));
        assert!(!scope.is_allowed(&outside));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_note_scope_allows_relative_assets_only_below_its_directory() {
        let root = test_directory("external-note-assets");
        let note_dir = root.join("shared note");
        let assets = note_dir.join("images");
        fs::create_dir_all(&assets).unwrap();
        let note = note_dir.join("note.md");
        let image = assets.join("image.png");
        let outside = root.join("outside.png");
        fs::write(&note, b"note").unwrap();
        fs::write(&image, b"image").unwrap();
        fs::write(&outside, b"outside").unwrap();

        let app = tauri::test::mock_app();
        allow_external_note_assets(app.handle(), &note).unwrap();
        let scope = app.asset_protocol_scope();

        assert!(scope.is_allowed(&note));
        assert!(scope.is_allowed(&image));
        assert!(!scope.is_allowed(&outside));

        fs::remove_dir_all(root).unwrap();
    }
}
