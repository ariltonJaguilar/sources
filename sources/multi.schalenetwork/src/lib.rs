#![no_std]
use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, FilterValue, Listing, ListingProvider, Manga,
	MangaPageResult, Page, PageContent, Result, Source,
	alloc::{String, Vec, format, string::ToString, vec},
	helpers::uri::encode_uri_component,
	imports::{net::Request, defaults::defaults_get},
	prelude::*,
};

mod models;
mod settings;

use models::{Books, ImagesInfo, MangaData, MangaDetail, select_quality};
use settings::{get_clearance, get_quality, get_remove_brackets};

const API: &str = "https://api.schale.network";
const ORIGIN: &str = "https://schale.network";

// Sort ID constants
const SORT_RECENTLY_POSTED: i32 = 4;
const SORT_TITLE: i32 = 2;
const SORT_PAGES: i32 = 3;
const SORT_MOST_VIEWED: i32 = 8;
const SORT_MOST_FAVORITED: i32 = 9;

// Category bitmask values
const CAT_MANGA: u32 = 2;
const CAT_DOUJINSHI: u32 = 4;
const CAT_ILLUSTRATION: u32 = 8;

struct SchaleNetwork;

// ---------------------------------------------------------------------------
// Source trait
// ---------------------------------------------------------------------------

impl Source for SchaleNetwork {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut sort = SORT_RECENTLY_POSTED;
		let mut cat_mask: u32 = 0;
		let mut include_terms: Vec<String> = Vec::new();
		let mut exclude_terms: Vec<String> = Vec::new();

		for filter in &filters {
			match filter {
				FilterValue::Sort { index, .. } => {
					sort = match index {
						0 => SORT_RECENTLY_POSTED,
						1 => SORT_TITLE,
						2 => SORT_PAGES,
						3 => SORT_MOST_VIEWED,
						4 => SORT_MOST_FAVORITED,
						_ => SORT_RECENTLY_POSTED,
					};
				}
				FilterValue::MultiSelect { id: _, included, .. } => {
					for name in included {
						cat_mask |= match name.as_str() {
							"Manga" => CAT_MANGA,
							"Doujinshi" => CAT_DOUJINSHI,
							"Illustration" => CAT_ILLUSTRATION,
							_ => 0,
						};
					}
				}
				FilterValue::Text { id, value } if !value.is_empty() => {
					let namespace = filter_id_to_namespace(id);
					if let Some(ns) = namespace {
						for token in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
							let exclude = token.starts_with('-');
							let term_str = if exclude { &token[1..] } else { token };
							let term = format!("{ns}:\"^{term_str}$\"");
							if exclude {
								exclude_terms.push(term);
							} else {
								include_terms.push(term);
							}
						}
					}
				}
				_ => {}
			}
		}

		// Query string: raw query text + filter terms
		let mut search_parts: Vec<String> = Vec::new();
		if let Some(ref q) = query {
			if !q.is_empty() {
				search_parts.push(q.clone());
			}
		}
		search_parts.extend(include_terms);

		let mut url = format!("{API}/books?page={page}&sort={sort}&limit=25");

		if !search_parts.is_empty() {
			url.push_str("&s=");
			url.push_str(&encode_uri_component(search_parts.join(" ")));
		}
		if !exclude_terms.is_empty() {
			url.push_str("&exclude=");
			url.push_str(&encode_uri_component(exclude_terms.join(" ")));
		}
		if cat_mask != 0 {
			url.push_str(&format!("&cat={cat_mask}"));
		}

		// Apply selected language as a language filter
		if let Some(lang) = defaults_get::<String>("selectedLanguage") {
			if lang != "All" && !lang.is_empty() {
				let lang_term = format!("language:\"^{}$\"", lang.to_lowercase());
				url.push_str("&s=");
				url.push_str(&encode_uri_component(lang_term));
			}
		}

		let books = Request::get(&url)?
			.header("Referer", &format!("{ORIGIN}/"))
			.header("Origin", ORIGIN)
			.json_owned::<Books>()?;

		let has_next_page = (books.page * books.limit) < books.total;
		let remove_brackets = get_remove_brackets();

		let entries = books
			.entries
			.into_iter()
			.map(|e| {
				let mut manga: Manga = e.into();
				if remove_brackets {
					manga.title = shorten_title_if(&manga.title);
				}
				manga
			})
			.collect();

		Ok(MangaPageResult {
			entries,
			has_next_page,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if !needs_details && !needs_chapters {
			return Ok(manga);
		}

		let (id, key) = split_key(&manga.key)?;
		let url = format!("{API}/books/detail/{id}/{key}");
		let detail = Request::get(&url)?
			.header("Referer", &format!("{ORIGIN}/"))
			.header("Origin", ORIGIN)
			.json_owned::<MangaDetail>()?;

		let remove_brackets = get_remove_brackets();

		if needs_details {
			let updated_manga = detail.to_manga(remove_brackets);
			manga.title = updated_manga.title;
			manga.cover = updated_manga.cover;
			manga.description = updated_manga.description;
			manga.authors = updated_manga.authors;
			manga.artists = updated_manga.artists;
			manga.tags = updated_manga.tags;
			manga.status = updated_manga.status;
			manga.content_rating = updated_manga.content_rating;
			manga.viewer = updated_manga.viewer;
			manga.update_strategy = updated_manga.update_strategy;
		}

		if needs_chapters {
			manga.chapters = Some(vec![Chapter {
				key: manga.key.clone(),
				title: Some("Chapter".into()),
				chapter_number: Some(1.0),
				date_uploaded: Some(detail.date()),
				url: Some(format!("{ORIGIN}/g/{id}/{key}")),
				..Default::default()
			}]);
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let (id, key) = split_key(&chapter.key)?;

		// POST to /books/detail/{id}/{key} → MangaData
		let data_url = format!("{API}/books/detail/{id}/{key}");
		let manga_data = Request::post(&data_url)?
			.header("Referer", &format!("{ORIGIN}/"))
			.header("Origin", ORIGIN)
			.json_owned::<MangaData>()?;

		let quality_pref = get_quality();
		let (data_key, quality_label) = select_quality(&manga_data.data, &quality_pref);

		let img_id = data_key.id.ok_or(error!("missing image data id"))?;
		let img_key = data_key.key.as_deref().ok_or(error!("missing image data key"))?;

		// Build image list URL, appending clearance token if available
		let clearance = get_clearance();
		let mut images_url =
			format!("{API}/books/data/{id}/{key}/{img_id}/{img_key}/{quality_label}");
		if let Some(ref crt) = clearance {
			images_url.push_str("?crt=");
			images_url.push_str(crt);
		}

		let images = Request::get(&images_url)?
			.header("Referer", &format!("{ORIGIN}/"))
			.header("Origin", ORIGIN)
			.json_owned::<ImagesInfo>()?;

		let pages = images
			.entries
			.into_iter()
			.enumerate()
			.map(|(_i, img)| {
				let url = format!("{}{}", images.base, img.path);
				let w = if quality_label == "0" {
					String::new()
				} else {
					quality_label.to_string()
				};
				let final_url = if w.is_empty() {
					url
				} else {
					format!("{url}?w={w}")
				};
				Page {
					content: PageContent::url(final_url),
					..Default::default()
				}
			})
			.collect();

		Ok(pages)
	}
}

// ---------------------------------------------------------------------------
// Listing provider (browse by sort)
// ---------------------------------------------------------------------------

impl ListingProvider for SchaleNetwork {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let sort = match listing.id.as_str() {
			"recently-posted" => SORT_RECENTLY_POSTED,
			"most-viewed" => SORT_MOST_VIEWED,
			"most-favorited" => SORT_MOST_FAVORITED,
			_ => SORT_RECENTLY_POSTED,
		};

		let url = format!("{API}/books?page={page}&sort={sort}&limit=25");
		let books = Request::get(&url)?
			.header("Referer", &format!("{ORIGIN}/"))
			.header("Origin", ORIGIN)
			.json_owned::<Books>()?;

		let has_next_page = (books.page * books.limit) < books.total;
		let remove_brackets = get_remove_brackets();

		let entries = books
			.entries
			.into_iter()
			.map(|e| {
				let mut manga: Manga = e.into();
				if remove_brackets {
					manga.title = shorten_title_if(&manga.title);
				}
				manga
			})
			.collect();

		Ok(MangaPageResult {
			entries,
			has_next_page,
		})
	}
}

// ---------------------------------------------------------------------------
// Deep link handler
// ---------------------------------------------------------------------------

impl DeepLinkHandler for SchaleNetwork {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		// Matches: https://schale.network/g/{id}/{key}
		const G_PATH: &str = "/g/";
		if let Some(idx) = url.find(G_PATH) {
			let path = &url[idx + G_PATH.len()..];
			// path = "{id}/{key}" or "{id}/{key}/..."
			let mut parts = path.split('/');
			if let (Some(id), Some(key)) = (parts.next(), parts.next()) {
				if !id.is_empty() && !key.is_empty() {
					let manga_key = format!("{id}/{key}");
					return Ok(Some(DeepLinkResult::Manga { key: manga_key }));
				}
			}
		}
		Ok(None)
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Split a manga key like "12345/abc123" into (id, key).
fn split_key(key: &str) -> Result<(&str, &str)> {
	let mut parts = key.splitn(2, '/');
	let id = parts.next().ok_or(error!("invalid manga key (no id)"))?;
	let k = parts.next().ok_or(error!("invalid manga key (no key)"))?;
	Ok((id, k))
}

/// Map a filter ID string to a Koharu API namespace tag name.
fn filter_id_to_namespace(id: &str) -> Option<&'static str> {
	match id {
		"artist" => Some("artist"),
		"circle" => Some("circle"),
		"parody" => Some("parody"),
		"magazine" => Some("magazine"),
		"character" => Some("character"),
		"cosplayer" => Some("cosplayer"),
		"pages" => Some("pages"),
		_ => None,
	}
}

fn shorten_title_if(title: &str) -> String {
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

register_source!(SchaleNetwork, ListingProvider, DeepLinkHandler);
