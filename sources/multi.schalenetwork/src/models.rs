use aidoku::{
	ContentRating, Manga, MangaStatus, UpdateStrategy, Viewer,
	alloc::{
		String, Vec,
		string::ToString,
	},
	prelude::*,
};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// List / browse responses
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct Books {
	pub entries: Vec<Entry>,
	pub total: i32,
	pub limit: i32,
	pub page: i32,
}

#[derive(Deserialize)]
pub struct Entry {
	pub id: i32,
	pub key: String,
	pub title: String,
	pub thumbnail: Thumbnail,
}

#[derive(Deserialize, Clone)]
pub struct Thumbnail {
	pub path: String,
}

// ---------------------------------------------------------------------------
// Manga detail response (GET /books/detail/{id}/{key})
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MangaDetail {
	pub id: i32,
	pub key: String,
	pub title: String,
	pub created_at: i64,
	pub updated_at: Option<i64>,
	pub thumbnails: Thumbnails,
	pub tags: Vec<Tag>,
}

#[derive(Deserialize)]
pub struct Thumbnails {
	pub base: String,
	pub main: Thumbnail,
	pub entries: Vec<Thumbnail>,
}

#[derive(Deserialize)]
pub struct Tag {
	pub name: String,
	pub namespace: i32,
}

// ---------------------------------------------------------------------------
// Page list response (POST /books/detail/{id}/{key})
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MangaData {
	pub data: ImageData,
}

#[derive(Deserialize)]
pub struct ImageData {
	#[serde(rename = "0")]
	pub original: DataKey,
	#[serde(rename = "780")]
	pub res780: Option<DataKey>,
	#[serde(rename = "980")]
	pub res980: Option<DataKey>,
	#[serde(rename = "1280")]
	pub res1280: Option<DataKey>,
	#[serde(rename = "1600")]
	pub res1600: Option<DataKey>,
}

#[derive(Deserialize)]
pub struct DataKey {
	pub id: Option<i32>,
	pub key: Option<String>,
}

// ---------------------------------------------------------------------------
// Image list response (GET /books/data/…)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ImagesInfo {
	pub base: String,
	pub entries: Vec<ImagePath>,
}

#[derive(Deserialize)]
pub struct ImagePath {
	pub path: String,
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

impl From<Entry> for Manga {
	fn from(e: Entry) -> Self {
		Manga {
			key: format!("{}/{}", e.id, e.key),
			title: e.title,
			cover: Some(e.thumbnail.path),
			url: Some(format!("https://niyaniya.moe/g/{}/{}", e.id, e.key)),
			content_rating: ContentRating::NSFW,
			status: MangaStatus::Completed,
			update_strategy: UpdateStrategy::Never,
			..Default::default()
		}
	}
}

impl MangaDetail {
	pub fn manga_key(&self) -> String {
		format!("{}/{}", self.id, self.key)
	}

	pub fn date(&self) -> i64 {
		self.updated_at.unwrap_or(self.created_at)
	}

	pub fn to_manga(&self, remove_brackets: bool) -> Manga {
		let title = if remove_brackets {
			shorten_title(&self.title)
		} else {
			self.title.clone()
		};

		// Separate tags by namespace
		let mut artists: Vec<String> = Vec::new();
		let mut circles: Vec<String> = Vec::new();
		let mut parodies: Vec<String> = Vec::new();
		let mut magazines: Vec<String> = Vec::new();
		let mut characters: Vec<String> = Vec::new();
		let mut cosplayers: Vec<String> = Vec::new();
		let mut uploaders: Vec<String> = Vec::new();
		let mut males: Vec<String> = Vec::new();
		let mut females: Vec<String> = Vec::new();
		let mut mixed: Vec<String> = Vec::new();
		let mut other_tags: Vec<String> = Vec::new();
		let mut genres: Vec<String> = Vec::new();

		for tag in &self.tags {
			let name = capitalize_each(&tag.name);
			match tag.namespace {
				0 => genres.push(name),
				1 => artists.push(name),
				2 => circles.push(name),
				3 => parodies.push(name),
				4 => magazines.push(name),
				5 => characters.push(name),
				6 => cosplayers.push(name),
				7 if tag.name != "anonymous" => uploaders.push(name),
				8 => males.push(format!("{name} ♂")),
				9 => females.push(format!("{name} ♀")),
				10 => mixed.push(name),
				12 => other_tags.push(name),
				_ => {}
			}
		}

		// Authors = circles first, fall back to artists
		let author_list = if !circles.is_empty() {
			circles.clone()
		} else {
			artists.clone()
		};

		// All genre/tag strings combined for the tags field
		let all_tags: Vec<String> = [
			&artists[..],
			&circles[..],
			&parodies[..],
			&magazines[..],
			&characters[..],
			&cosplayers[..],
			&genres[..],
			&females[..],
			&males[..],
			&mixed[..],
			&other_tags[..],
		]
		.iter()
		.flat_map(|s| s.iter().cloned())
		.collect();

		// Build description
		let mut desc = String::new();
		append_section(&mut desc, "Circles", &circles);
		append_section(&mut desc, "Uploaders", &uploaders);
		append_section(&mut desc, "Magazines", &magazines);
		append_section(&mut desc, "Cosplayers", &cosplayers);
		append_section(&mut desc, "Parodies", &parodies);
		append_section(&mut desc, "Characters", &characters);
		if !desc.is_empty() {
			desc.push('\n');
		}
		let page_count = self.thumbnails.entries.len();
		desc.push_str(&format!("Pages: {page_count}\n"));

		let cover = Some(format!("{}{}", self.thumbnails.base, self.thumbnails.main.path));

		Manga {
			key: self.manga_key(),
			title,
			cover,
			description: if desc.is_empty() { None } else { Some(desc.trim_end().to_string()) },
			url: Some(format!("https://schale.network/g/{}/{}", self.id, self.key)),
			authors: if author_list.is_empty() { None } else { Some(author_list) },
			artists: if artists.is_empty() { None } else { Some(artists) },
			tags: if all_tags.is_empty() { None } else { Some(all_tags) },
			content_rating: ContentRating::NSFW,
			status: MangaStatus::Completed,
			viewer: Viewer::RightToLeft,
			update_strategy: UpdateStrategy::Never,
			..Default::default()
		}
	}
}

// ---------------------------------------------------------------------------
// Quality selection
// ---------------------------------------------------------------------------

/// Returns the `DataKey` and quality label that best matches the preference,
/// falling back through available qualities.
pub fn select_quality<'a>(data: &'a ImageData, preferred: &str) -> (&'a DataKey, &'static str) {
	macro_rules! try_key {
		($opt:expr, $label:literal) => {
			if let Some(ref k) = $opt {
				if k.id.is_some() {
					return (k, $label);
				}
			}
		};
	}

	match preferred {
		"1600" => {
			try_key!(data.res1600, "1600");
			try_key!(data.res1280, "1280");
			try_key!(data.res980, "980");
			try_key!(data.res780, "780");
		}
		"1280" => {
			try_key!(data.res1280, "1280");
			try_key!(data.res1600, "1600");
			try_key!(data.res980, "980");
			try_key!(data.res780, "780");
		}
		"980" => {
			try_key!(data.res980, "980");
			try_key!(data.res1280, "1280");
			try_key!(data.res1600, "1600");
			try_key!(data.res780, "780");
		}
		"780" => {
			try_key!(data.res780, "780");
			try_key!(data.res980, "980");
			try_key!(data.res1280, "1280");
			try_key!(data.res1600, "1600");
		}
		_ => {} // "0" = always use original below
	}
	(&data.original, "0")
}

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

fn shorten_title(title: &str) -> String {
	// Remove anything in [], (), {} — mirrors Kotlin's shortenTitleRegex
	let mut result = String::new();
	let mut depth = 0usize;
	for ch in title.chars() {
		match ch {
			'[' | '(' | '{' => depth += 1,
			']' | ')' | '}' => {
				if depth > 0 {
					depth -= 1;
				}
			}
			_ if depth == 0 => result.push(ch),
			_ => {}
		}
	}
	result.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn capitalize_each(s: &str) -> String {
	s.split(' ')
		.map(|word| {
			let mut chars = word.chars();
			match chars.next() {
				None => String::new(),
				Some(first) => {
					let upper: String = first.to_uppercase().collect();
					upper + chars.as_str()
				}
			}
		})
		.collect::<Vec<_>>()
		.join(" ")
}

fn append_section(desc: &mut String, label: &str, items: &[String]) {
	if !items.is_empty() {
		desc.push_str(label);
		desc.push_str(": ");
		desc.push_str(&items.join(", "));
		desc.push('\n');
	}
}
