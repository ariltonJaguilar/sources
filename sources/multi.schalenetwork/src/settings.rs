use aidoku::{
	alloc::String,
	imports::defaults::defaults_get,
};

/// Preferred image resolution (e.g. "1280", "1600", "980", "780", "0").
pub fn get_quality() -> String {
	defaults_get::<String>("imageQuality").unwrap_or_else(|| "1280".into())
}

/// Optional clearance token for page access. Empty if not set.
pub fn get_clearance() -> Option<String> {
	defaults_get::<String>("clearanceToken")
		.filter(|s| !s.is_empty())
}

/// Whether to strip [] and () from titles.
pub fn get_remove_brackets() -> bool {
	defaults_get::<bool>("removeAdditionalInfo").unwrap_or(false)
}
