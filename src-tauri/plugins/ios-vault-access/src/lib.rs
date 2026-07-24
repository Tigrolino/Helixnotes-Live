use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, PluginApi, PluginHandle, TauriPlugin},
    AppHandle, Manager, Runtime,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSelection {
    pub bookmark_id: String,
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub cancelled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BookmarkRequest<'a> {
    bookmark_id: &'a str,
}

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_ios_vault_access);

pub struct IosVaultAccess<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> IosVaultAccess<R> {
    pub fn choose_folder(&self) -> Result<FolderSelection, String> {
        self.0
            .run_mobile_plugin("chooseFolder", ())
            .map_err(|error| error.to_string())
    }

    pub fn resolve_folder(&self, bookmark_id: &str) -> Result<FolderSelection, String> {
        self.0
            .run_mobile_plugin("resolveFolder", BookmarkRequest { bookmark_id })
            .map_err(|error| error.to_string())
    }

    pub fn release_active(&self) -> Result<(), String> {
        self.0
            .run_mobile_plugin("releaseActive", ())
            .map_err(|error| error.to_string())
    }

    pub fn commit_staged(&self) -> Result<(), String> {
        self.0
            .run_mobile_plugin("commitStaged", ())
            .map_err(|error| error.to_string())
    }

    pub fn rollback_staged(&self) -> Result<(), String> {
        self.0
            .run_mobile_plugin("rollbackStaged", ())
            .map_err(|error| error.to_string())
    }

    pub fn forget_bookmark(&self, bookmark_id: &str) -> Result<(), String> {
        self.0
            .run_mobile_plugin("forgetBookmark", BookmarkRequest { bookmark_id })
            .map_err(|error| error.to_string())
    }
}

pub trait IosVaultAccessExt<R: Runtime> {
    fn ios_vault_access(&self) -> &IosVaultAccess<R>;
}

impl<R: Runtime, T: Manager<R>> IosVaultAccessExt<R> for T {
    fn ios_vault_access(&self) -> &IosVaultAccess<R> {
        self.state::<IosVaultAccess<R>>().inner()
    }
}

fn initialize<R: Runtime>(
    _app: &AppHandle<R>,
    api: PluginApi<R, ()>,
) -> tauri::Result<IosVaultAccess<R>> {
    let handle = api.register_ios_plugin(init_plugin_ios_vault_access)?;
    Ok(IosVaultAccess(handle))
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("ios-vault-access")
        .setup(|app, api| {
            let access = initialize(app, api)?;
            app.manage(access);
            Ok(())
        })
        .build()
}
