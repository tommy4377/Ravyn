//! Persistent library organization and content classification.

mod category;
mod cleanup;
mod move_root;
mod root;
mod scan;
mod template;
mod trash;

pub use category::{
    LibraryCategory, classify_file, classify_file_with_overrides, classify_name,
    classify_name_with_overrides, validate_category_overrides,
};
pub use move_root::{
    LibraryMoveConflictPolicy, LibraryMovePreflight, LibraryMoveRequest, LibraryMoveState,
    LibraryMoveStatus, cancel_library_move, execute_library_move, finalize_activated_library_move,
    preflight_library_move, recover_interrupted_library_move, start_library_move,
};
pub(crate) use move_root::{LibraryMoveItemRecord, LibraryMovePlan};
pub use root::{LIBRARY_DIRECTORIES, category_directory, prepare_library_layout};

pub use cleanup::{
    ActivityBucket, CategoryStatistics, CleanupPolicies, CleanupReport, PersonalStatistics,
    run_cleanup,
};
pub use scan::{
    LibraryImportRequest, LibraryImportStatus, RelocationReport, RelocationRequest,
    SharedImportStatus, VerifyLibraryReport, import_directory, repair_relocations, reserve_import,
    verify_entries,
};
pub use template::{TemplatePreview, TemplatePreviewRequest, render as render_template};
pub use trash::{move_to_trash, purge as purge_entry, restore as restore_entry};
