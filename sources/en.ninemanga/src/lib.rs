#![no_std]
use aidoku::{
	Chapter, FilterValue, Listing, ListingProvider, Manga, MangaPageResult, MangaStatus, Page,
	PageContent, Result, Source,
	alloc::{String, Vec, string::ToString, vec},
	helpers::uri::encode_uri_component,
	imports::{
		html::Document,
		net::Request,
		std::parse_date_with_options,
	},
	prelude::*,
};

const BASE_URL: &str = "https://www.ninemanga.com";

struct NineManga;

impl Source for NineManga {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let url = if let Some(ref q) = query {
			format!(
				"{BASE_URL}/search/?wd={}&page={}&type=high",
				encode_uri_component(q),
				page
			)
		} else {
			let mut parts: Vec<String> = Vec::new();
			parts.push(format!("page={page}"));
			parts.push("type=high".into());

			for filter in filters {
				match filter {
					FilterValue::Select { id, value } => {
						// completed_series filter
						parts.push(format!("{}={}", id, encode_uri_component(&value)));
					}
					FilterValue::MultiSelect {
						included, excluded, ..
					} => {
						// genres filter — map to category_id / out_category_id
						if !included.is_empty() {
							let ids = included.join(",") + ",";
							parts.push(format!("category_id={}", encode_uri_component(&ids)));
						}
						if !excluded.is_empty() {
							let ids = excluded.join(",") + ",";
							parts.push(format!("out_category_id={}", encode_uri_component(&ids)));
						}
					}
					_ => {}
				}
			}

			format!("{BASE_URL}/search/?{}", parts.join("&"))
		};

		let html = Request::get(url)?.html()?;
		parse_manga_list(&html)
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		// append ?waring=1 to bypass adult-content warning page
		let url = format!("{BASE_URL}{}?waring=1", manga.key);
		let html = Request::get(&url)?.html()?;

		if needs_details {
			if let Some(bookintro) = html.select_first("div.bookintro") {
				manga.title = bookintro
					.select_first("li > span:not([class])")
					.and_then(|el| el.text())
					.map(|s| {
						let s = s.trim_end_matches(" Manga");
						s.to_string()
					})
					.unwrap_or(manga.title);

				manga.authors = bookintro
					.select_first("li a[itemprop=author]")
					.and_then(|el| el.text())
					.map(|s| vec![s]);

				manga.tags = bookintro
					.select("li[itemprop=genre] a")
					.map(|els| els.filter_map(|el| el.text()).collect());

				manga.status = bookintro
					.select_first("li a.red")
					.and_then(|el| el.text())
					.map(|s| parse_status(&s))
					.unwrap_or_default();

				manga.description = bookintro
					.select_first("p[itemprop=description]")
					.and_then(|el| el.text());

				manga.cover = bookintro
					.select_first("img[itemprop=image]")
					.and_then(|el| el.attr("abs:src"));
			}
			manga.url = Some(format!("{BASE_URL}{}", manga.key));
		}

		if needs_chapters {
			manga.chapters = html
				.select("ul.sub_vol_ul > li")
				.map(|els| {
					els.filter_map(|el| {
						let link = el.select_first("a.chapter_list_a")?;
						let href = link.attr("abs:href")?;
						// strip base URL to get relative path; replace %20 with space
						let key = href
							.strip_prefix(BASE_URL)
							.map(|s| s.replace("%20", " "))
							.unwrap_or_else(|| href.replace("%20", " "));
						let title = link.text()?;
						let date = el
							.select_first("span")
							.and_then(|e| e.text())
							.and_then(|s| {
								parse_date_with_options(s, "MMM d, yyyy", "en_US", "current")
							});
						Some(Chapter {
							key,
							title: Some(title),
							date_uploaded: date,
							url: Some(href),
							..Default::default()
						})
					})
					.collect()
				});
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{BASE_URL}{}", chapter.key);
		let html = Request::get(&url)?.html()?;

		// Each option value is a relative path for one page of the chapter
		let page_urls: Vec<String> = html
			.select("select#page option")
			.map(|els| {
				els.filter_map(|el| el.attr("value").map(|v| format!("{BASE_URL}{v}")))
					.collect()
			})
			.unwrap_or_default();

		if page_urls.is_empty() {
			return Err(error!("No pages found in chapter"));
		}

		// Fetch each page and extract the actual image URL
		page_urls
			.into_iter()
			.map(|page_url| {
				let page_html = Request::get(&page_url)?.html()?;
				let image_url = page_html
					.select_first("div.pic_box img.manga_pic")
					.and_then(|el| el.attr("abs:src"))
					.ok_or_else(|| error!("No image found on page"))?;
				Ok(Page {
					content: PageContent::url(image_url),
					..Default::default()
				})
			})
			.collect()
	}
}

impl ListingProvider for NineManga {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let url = match listing.id.as_str() {
			"latest" => format!("{BASE_URL}/list/New-Update/"),
			"hot" => format!("{BASE_URL}/list/Hot-Book/"),
			"new" => format!("{BASE_URL}/list/New-Book/"),
			// "popular" and fallback — paginated directory
			_ => format!("{BASE_URL}/category/index_{page}.html"),
		};

		let html = Request::get(url)?.html()?;
		parse_manga_list(&html)
	}
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn parse_manga_list(html: &Document) -> Result<MangaPageResult> {
	let entries: Vec<Manga> = html
		.select("dl.bookinfo")
		.map(|els| {
			els.filter_map(|el| {
				let link = el.select_first("a.bookname")?;
				let href = link.attr("abs:href")?;
				let key = href
					.strip_prefix(BASE_URL)
					.map(|s| s.to_string())
					.unwrap_or_else(|| href.clone());
				Some(Manga {
					key,
					title: link.text()?,
					cover: el.select_first("img").and_then(|img| img.attr("abs:src")),
					url: Some(href),
					..Default::default()
				})
			})
			.collect()
		})
		.unwrap_or_default();

	let has_next_page = html
		.select_first("ul.pageList > li:last-child > a.l")
		.is_some();

	Ok(MangaPageResult {
		entries,
		has_next_page,
	})
}

fn parse_status(s: &str) -> MangaStatus {
	if s.contains("Ongoing") {
		MangaStatus::Ongoing
	} else if s.contains("Completed") {
		MangaStatus::Completed
	} else {
		MangaStatus::Unknown
	}
}

register_source!(NineManga, ListingProvider);
