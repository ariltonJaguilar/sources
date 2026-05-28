#![no_std]
use aidoku::{
	Chapter, ContentRating, DeepLinkHandler, DeepLinkResult, FilterValue, Manga, MangaPageResult,
	MangaStatus, Page, PageContent, Result, Source, UpdateStrategy, Viewer,
	alloc::{String, Vec, string::ToString, vec},
	helpers::uri::encode_uri_component,
	imports::{
		html::Document,
		net::Request,
		std::parse_date,
	},
	prelude::*,
};
use serde::Deserialize;

mod decrypt;

const BASE_URL: &str = "https://hentainexus.com";

struct HentaiNexus;

// ---------------------------------------------------------------------------
// DTOs for decrypted page JSON
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ReaderPage {
	#[serde(rename = "type")]
	entry_type: String,
	image: Option<String>,
}

// ---------------------------------------------------------------------------
// Source implementation
// ---------------------------------------------------------------------------

impl Source for HentaiNexus {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut q_parts: Vec<String> = Vec::new();

		// Parse text filters (tag, artist, author, circle, event, parody,
		// magazine, publisher). Each value may be comma-separated; a token
		// prefixed with '-' excludes it.
		for filter in &filters {
			if let FilterValue::Text { id, value } = filter {
				if value.is_empty() {
					continue;
				}
				for token in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
					let exclude = token.starts_with('-');
					let text = if exclude { token[1..].trim() } else { token };
					if text.is_empty() {
						continue;
					}
					let prefix = if exclude { "-" } else { "" };
					let part = if text.contains(' ') {
						format!("{}{}:\"{}\"", prefix, id, text)
					} else {
						format!("{}{}:{}", prefix, id, text)
					};
					q_parts.push(part);
				}
			}
		}

		if let Some(ref q) = query {
			if !q.is_empty() {
				q_parts.push(q.clone());
			}
		}

		let url = build_url(&q_parts, page);

		let html = Request::get(&url)?
			.header("Referer", &format!("{BASE_URL}/"))
			.html()?;

		Ok(parse_manga_list(html))
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

		let url = format!("{BASE_URL}/view/{}", manga.key);
		let html = Request::get(&url)?
			.header("Referer", &format!("{BASE_URL}/"))
			.html()?;

		if needs_details {
			manga.title = html
				.select_first("h1.title")
				.and_then(|el| el.text())
				.unwrap_or(manga.title);

			manga.cover = html
				.select_first("figure.image img")
				.and_then(|el| el.attr("abs:src"));

			if let Some(table) = html.select_first(".view-page-details") {
				// Authors and artists
				let artists: Vec<String> = table
					.select("td.viewcolumn:contains(Artist) + td a")
					.map(|els| els.filter_map(|el| el.text()).collect())
					.unwrap_or_default();

				let authors: Vec<String> = table
					.select("td.viewcolumn:contains(Author) + td a")
					.map(|els| els.filter_map(|el| el.text()).collect())
					.unwrap_or_default();

				// Combine authors + artists (deduplicated), matching Mihon logic
				let mut combined = authors.clone();
				for a in &artists {
					if !combined.contains(a) {
						combined.push(a.clone());
					}
				}
				if !combined.is_empty() {
					manga.authors = Some(combined);
				}
				if !artists.is_empty() {
					manga.artists = Some(artists);
				}

				// Tags – strip trailing "(N,NNN)" count from each tag name
				manga.tags = table
					.select("span.tag a")
					.map(|els| {
						els.filter_map(|el| el.text())
							.map(|t| strip_tag_count(&t).to_string())
							.collect::<Vec<_>>()
					});

				// Description – metadata table + optional description text
				let meta_keys = [
					"Circle", "Event", "Magazine", "Parody", "Publisher", "Pages", "Favorites",
				];
				let mut desc_lines: Vec<String> = Vec::new();
				for key in &meta_keys {
					let selector = format!("td.viewcolumn:contains({key}) + td");
					if let Some(cell) = table.select_first(&selector) {
						let text = cell
							.text()
							.filter(|s| !s.is_empty())
							.or_else(|| cell.select_first("a").and_then(|a| a.text()));
						if let Some(text) = text {
							desc_lines.push(format!("{key}: {text}"));
						}
					}
				}
				if let Some(desc_cell) =
					table.select_first("td.viewcolumn:contains(Description) + td")
				{
					if let Some(text) = desc_cell.text() {
						desc_lines.push(String::new());
						desc_lines.push(text);
					}
				}
				if !desc_lines.is_empty() {
					manga.description = Some(desc_lines.join("\n"));
				}
			}

			manga.status = MangaStatus::Completed;
			manga.content_rating = ContentRating::NSFW;
			manga.viewer = Viewer::RightToLeft;
			manga.update_strategy = UpdateStrategy::Never;
		}

		if needs_chapters {
			let date_uploaded = html
				.select_first(".view-page-details")
				.and_then(|table| {
					table
						.select_first("td.viewcolumn:contains(Published) + td")
						.and_then(|el| el.text())
				})
				.and_then(|s| parse_date(s, "dd MMMM yyyy"));

			manga.chapters = Some(vec![Chapter {
				key: manga.key.clone(),
				title: Some("Chapter".into()),
				chapter_number: Some(1.0),
				date_uploaded,
				url: Some(format!("{BASE_URL}/read/{}", manga.key)),
				..Default::default()
			}]);
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{BASE_URL}/read/{}", chapter.key);
		let html = Request::get(&url)?
			.header("Referer", &format!("{BASE_URL}/"))
			.html()?;

		// Find the <script> that contains the initReader call
		let scripts = html
			.select("script")
			.ok_or(error!("no <script> elements found"))?;

		for script in scripts {
			let Some(data) = script.data() else { continue };
			if !data.contains("initReader") {
				continue;
			}

			// Extract the base64 argument: initReader("<encoded>", ...)
			let marker = "initReader(\"";
			let Some(start) = data.find(marker).map(|i| i + marker.len()) else {
				continue;
			};
			let rest = &data[start..];
			let end = rest.find("\",").unwrap_or(rest.len());
			let encoded = &rest[..end];

			// Decrypt and parse the JSON array
			let decrypted = decrypt::decrypt_pages(encoded)?;
			let items: Vec<ReaderPage> = serde_json::from_str(&decrypted)
				.map_err(|_| error!("failed to parse page JSON"))?;

			return Ok(items
				.into_iter()
				.filter(|p| p.entry_type == "image")
				.filter_map(|p| {
					let image_url = p.image?;
					Some(Page {
						content: PageContent::url(image_url),
						..Default::default()
					})
				})
				.collect());
		}

		Err(error!("initReader script not found; site structure may have changed"))
	}
}

// ---------------------------------------------------------------------------
// Deep link handler
// ---------------------------------------------------------------------------

impl DeepLinkHandler for HentaiNexus {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		const VIEW_PATH: &str = "/view/";
		if let Some(idx) = url.find(VIEW_PATH) {
			let id_part = &url[idx + VIEW_PATH.len()..];
			let id = id_part.split('/').next().unwrap_or(id_part);
			if !id.is_empty() {
				return Ok(Some(DeepLinkResult::Manga { key: id.into() }));
			}
		}
		Ok(None)
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the request URL from filter query parts and page number.
fn build_url(q_parts: &[String], page: i32) -> String {
	if q_parts.is_empty() {
		// Browsing the popular / homepage listing
		if page > 1 {
			format!("{BASE_URL}/page/{page}")
		} else {
			BASE_URL.to_string()
		}
	} else {
		let q = encode_uri_component(q_parts.join(" "));
		if page > 1 {
			format!("{BASE_URL}/page/{page}?q={q}")
		} else {
			format!("{BASE_URL}?q={q}")
		}
	}
}

/// Parse a gallery listing page (home / search results).
fn parse_manga_list(html: Document) -> MangaPageResult {
	let entries = html
		.select(".container .column")
		.map(|els| {
			els.filter_map(|el| {
				let a = el.select_first("a")?;
				let href = a.attr("abs:href")?;
				// Key = numeric ID extracted from /view/{id}
				let key = href
					.strip_prefix(BASE_URL)
					.and_then(|s| s.strip_prefix("/view/"))
					.map(|s| s.split('/').next().unwrap_or(s))
					.map(String::from)?;
				let title = el.select_first(".card-header-title")?.text()?;
				let cover = el
					.select_first(".card-image img")
					.and_then(|img| img.attr("abs:src"));
				Some(Manga {
					key,
					title,
					cover,
					url: Some(href),
					content_rating: ContentRating::NSFW,
					status: MangaStatus::Completed,
					viewer: Viewer::RightToLeft,
					update_strategy: UpdateStrategy::Never,
					..Default::default()
				})
			})
			.collect::<Vec<_>>()
		})
		.unwrap_or_default();

	let has_next_page = html.select_first("a.pagination-next[href]").is_some();

	MangaPageResult {
		entries,
		has_next_page,
	}
}

/// Remove trailing tag-count annotation like " (12,345)" from a tag name.
fn strip_tag_count(s: &str) -> &str {
	let s = s.trim();
	if s.ends_with(')') {
		if let Some(paren_start) = s.rfind('(') {
			let between = &s[paren_start + 1..s.len() - 1];
			if between.chars().all(|c| c.is_ascii_digit() || c == ',') {
				return s[..paren_start].trim_end();
			}
		}
	}
	s
}

register_source!(HentaiNexus, DeepLinkHandler);
