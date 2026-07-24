use std::path::PathBuf;

use cxx_qt_lib::QString;

use simsapa_backend::logger::info;
use simsapa_backend::{get_create_simsapa_internal_app_root, save_to_file};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("utils.h");
        fn get_app_data_storage_paths_json() -> QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[namespace = "storage_manager"]
        type StorageManager = super::StorageManagerRust;

        #[qinvokable]
        fn get_app_data_storage_paths_json(self: &StorageManager) -> QString;

        #[qinvokable]
        fn save_storage_path(self: &StorageManager, path: &QString, is_internal: bool);
    }
}

#[derive(Default)]
pub struct StorageManagerRust {}


impl qobject::StorageManager {
    pub fn get_app_data_storage_paths_json(&self) -> QString {
        qobject::get_app_data_storage_paths_json()
    }

    /// Save the storage path selected with the StorageDialog.
    pub fn save_storage_path(&self, selected_path: &QString, is_internal: bool) {
        // Write storage_path.txt to the internal storage, in the folder returned
        // by get_create_simsapa_internal_app_root()
        //
        // On Android the path does not include '.local/share/simsapa':
        // /data/user/0/io.github.simsapa.app/files/storage_path.txt
        //
        // Values returned from accepting the StorageDialog:
        //
        // Linux:
        // /home/gambhiro/.local/share/simsapa, is_internal: true
        //
        // Android:
        // /data/user/0/io.github.simsapa.app/files, is_internal: true
        // /storage/emulated/0/Android/data/io.github.simsapa.app/files, is_internal: false
        info(&format!("Selected path: {}, is_internal: {}", selected_path, is_internal));

        let internal_app_root = if let Ok(p) = get_create_simsapa_internal_app_root() {
            p
        } else {
            PathBuf::from(".")
        };

        let save_path = internal_app_root.join("storage-path.txt");
        let msg = save_to_file(selected_path.to_string().as_bytes(), save_path.to_str().unwrap_or_default());
        info(&msg);
    }
}
