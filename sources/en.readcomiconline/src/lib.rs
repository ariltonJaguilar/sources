#![no_std]
use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, FilterValue, Home, HomeComponent, HomeComponentValue,
	HomeLayout, Listing, ListingProvider, Manga, MangaPageResult, MangaStatus, Page, PageContent,
	Result, Source, Viewer,
	alloc::{String, Vec, string::ToString, vec},
	helpers::{
		string::StripPrefixOrSelf,
		uri::{QueryParameters, encode_uri_component},
	},
	imports::{
		html::Document,
		js::JsContext,
		net::Request,
		std::{parse_date, send_partial_result},
	},
	prelude::*,
};
use serde::Deserialize;

const BASE_URL: &str = "https://readcomiconline.li";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
const REMOTE_CONFIG_URL: &str = "https://raw.githubusercontent.com/keiyoushi/extensions-source/refs/heads/main/src/en/readcomiconline/config.json";

#[derive(Deserialize)]
struct RemoteConfig {
	#[serde(rename = "imageDecryptEval")]
	image_decrypt_eval: String,
}

struct ReadComicOnline;

impl Source for ReadComicOnline {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let url = if let Some(ref query) = query {
			let mut qs = QueryParameters::new();
			qs.push("page", Some(&page.to_string()));
			qs.push("comicName", Some(query));

			for filter in &filters {
				match filter {
					FilterValue::Select { id, value } => {
						qs.push(id, Some(value));
					}
					FilterValue::MultiSelect {
						included, excluded, ..
					} => {
						fn genre_id(genre: &str) -> &'static str {
							// [...document.querySelectorAll("ul#genres > li")]
							// 	.map((el) => `"${el.querySelector("label").textContent.trim()}" => "${el.querySelector("select").getAttribute("gid")}"`)
							// 	.join(",")
							// on https://readcomiconline.li/AdvanceSearch
							match genre {
								"Action" => "1",
								"Adventure" => "2",
								"Anthology" => "38",
								"Anthropomorphic" => "46",
								"Biography" => "41",
								"Children" => "49",
								"Comedy" => "3",
								"Crime" => "17",
								"Drama" => "19",
								"Family" => "25",
								"Fantasy" => "20",
								"Fighting" => "31",
								"Graphic Novels" => "5",
								"Historical" => "28",
								"Horror" => "15",
								"Leading Ladies" => "35",
								"LGBTQ" => "51",
								"Literature" => "44",
								"Manga" => "40",
								"Martial Arts" => "4",
								"Mature" => "8",
								"Military" => "33",
								"Mini-Series" => "56",
								"Movies & TV" => "47",
								"Music" => "55",
								"Mystery" => "23",
								"Mythology" => "21",
								"Personal" => "48",
								"Political" => "42",
								"Post-Apocalyptic" => "43",
								"Psychological" => "27",
								"Pulp" => "39",
								"Religious" => "53",
								"Robots" => "9",
								"Romance" => "32",
								"School Life" => "52",
								"Sci-Fi" => "16",
								"Slice of Life" => "50",
								"Sport" => "54",
								"Spy" => "30",
								"Superhero" => "22",
								"Supernatural" => "24",
								"Suspense" => "29",
								"Teen" => "57",
								"Thriller" => "18",
								"Vampires" => "34",
								"Video Games" => "37",
								"War" => "26",
								"Western" => "45",
								"Zombies" => "36",
								_ => "",
							}
						}
						qs.push(
							"ig",
							Some(
								&included
									.iter()
									.map(|s| genre_id(s))
									.collect::<Vec<_>>()
									.join(","),
							),
						);
						qs.push(
							"eg",
							Some(
								&excluded
									.iter()
									.map(|s| genre_id(s))
									.collect::<Vec<_>>()
									.join(","),
							),
						);
					}
					_ => {}
				}
			}

			format!("{BASE_URL}/AdvanceSearch?{qs}")
		} else {
			let mut path = "ComicList".to_string();
			let mut sort = "MostPopular";

			for filter in &filters {
				match filter {
					FilterValue::Text { id, value } => {
						let value = value.replace(" ", "-");
						if id == "author" {
							path = format!("Writer/{}", encode_uri_component(value));
						} else if id == "artist" {
							path = format!("Artist/{}", encode_uri_component(value));
						}
					}
					FilterValue::Sort { index, .. } => {
						sort = match index {
							0 => "",
							1 => "MostPopular",
							2 => "LatestUpdate",
							3 => "Newest",
							_ => "",
						}
					}
					FilterValue::MultiSelect { included, .. } => {
						if let Some(genre) = included.first() {
							let encoded = genre.replace(" & ", "-").replace(" ", "-");
							path = format!("Genre/{encoded}");
						}
					}
					_ => {}
				}
			}

			format!("{BASE_URL}/{path}/{sort}?page={page}")
		};

		let html = Request::get(&url)?
			.header("Referer", &format!("{BASE_URL}/"))
			.header("User-Agent", USER_AGENT)
			.html()?;
		Ok(parse_comic_list(html))
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let url = format!("{BASE_URL}{}", manga.key);
		let html = Request::get(&url)?
			.header("Referer", &format!("{BASE_URL}/"))
			.header("User-Agent", USER_AGENT)
			.html()?;

		if needs_details {
			let info_element = html
				.select_first("div.barContent")
				.ok_or(error!("missing info element"))?;

			manga.title = info_element
				.select_first("a.bigChar")
				.and_then(|el| el.text())
				.unwrap_or(manga.title);
			manga.cover = html
				.select_first(".rightBox:eq(0) img")
				.and_then(|el| el.attr("abs:src"));
			manga.authors = info_element
				.select_first("p:has(span:contains(Writer:)) > a")
				.and_then(|el| el.text())
				.map(|str| vec![str]);
			manga.artists = info_element
				.select_first("p:has(span:contains(Artist:)) > a")
				.and_then(|el| el.text())
				.map(|str| vec![str]);
			manga.description = info_element
				.select_first("p:has(span:contains(Summary:)) ~ p")
				.and_then(|el| el.text());
			manga.tags = info_element
				.select("p:has(span:contains(Genres:)) > a")
				.map(|els| els.filter_map(|el| el.text()).collect::<Vec<_>>());
			manga.status = info_element
				.select_first("p:has(span:contains(Status:))")
				.and_then(|el| el.text())
				.map(|str| {
					if str.contains("Ongoing") {
						MangaStatus::Ongoing
					} else if str.contains("Completed") {
						MangaStatus::Completed
					} else {
						MangaStatus::Unknown
					}
				})
				.unwrap_or_default();
			manga.viewer = Viewer::LeftToRight;

			if needs_chapters {
				send_partial_result(&manga);
			}
		}

		if needs_chapters {
			manga.chapters = html.select("table.listing tr:gt(1)").map(|els| {
				els.filter_map(|el| {
					let url_element = el.select_first("a")?;
					let url = url_element.attr("abs:href")?;

					let mut chapter_number = None;
					let title = url_element.text().map(|text| {
						// remove series title prefix from chapter title
						let text = text.strip_prefix_or_self(&manga.title).trim();
						// parse chapter number after '#' (e.g. Issue #10)
						if let Some(idx) = text.find('#') {
							chapter_number = text[idx + 1..].parse::<f32>().ok();
						}
						text.into()
					});

					Some(Chapter {
						key: url.strip_prefix(BASE_URL)?.into(),
						title,
						chapter_number,
						date_uploaded: el
							.select_first("td:eq(1)")
							.and_then(|el| el.text())
							.and_then(|str| parse_date(str, "MM/dd/yyyy")),
						url: Some(url),
						..Default::default()
					})
				})
				.collect()
			})
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		// Fetch remote decrypt config (keiyoushi maintains this)
		let config = Request::get(REMOTE_CONFIG_URL)?
			.json_owned::<RemoteConfig>()?;

		// Append quality + server suffix as Kotlin does: &s=&quality=hq&readType=1
		let url = format!("{BASE_URL}{}&s=&quality=hq&readType=1", chapter.key);
		let html = Request::get(url)?
			.header("Referer", &format!("{BASE_URL}/"))
			.header("User-Agent", USER_AGENT)
			.html()?;

		let scripts = html
			.select("script")
			.ok_or(error!("html select `script` failed"))?;

		let mut links: Vec<String> = Vec::new();

		for script in scripts {
			let Some(data) = script.data().and_then(|s| {
				let s = s.trim();
				if s.is_empty() {
					return None;
				}
				serde_json::to_string(&s).ok()
			}) else {
				continue;
			};

			let js_string =
				format!("let _encryptedString = {data};let _useServer2 = false;{}", config.image_decrypt_eval);
			let Ok(result) = JsContext::new().eval(&js_string) else {
				continue;
			};

			if let Ok(new_links) = serde_json::from_str::<Vec<String>>(&result) {
				links.extend(new_links.into_iter().filter(|s| !s.is_empty()));
			}
		}

		Ok(links
			.into_iter()
			.map(|link| Page {
				content: PageContent::url(link),
				..Default::default()
			})
			.collect())
	}
}

impl ListingProvider for ReadComicOnline {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let url = format!("{BASE_URL}/{}?page={page}", listing.id);
		let html = Request::get(url)?
			.header("Referer", &format!("{BASE_URL}/"))
			.header("User-Agent", USER_AGENT)
			.html()?;
		Ok(parse_comic_list(html))
	}
}

fn parse_comic_list(html: Document) -> MangaPageResult {
	let entries = html
		.select(".section.group.list")
		.map(|elements| {
			elements
				.filter_map(|element| {
					let cover_anchor = element.select_first(".col.cover > a")?;
					let url = cover_anchor.attr("abs:href")?;
					let key = url.strip_prefix(BASE_URL).map(String::from)?;
					let cover = cover_anchor.select_first("img")?.attr("abs:src");
					let title = element
						.select_first(".col.info > p > a")
						.and_then(|el| el.text())
						.unwrap_or_default();
					Some(Manga {
						key,
						title,
						cover,
						url: Some(url),
						..Default::default()
					})
				})
				.collect::<Vec<Manga>>()
		})
		.unwrap_or_default();

	let has_next_page = html.select("a.right_bt.next_bt").is_some();

	MangaPageResult {
		entries,
		has_next_page,
	}
}

impl Home for ReadComicOnline {
	fn get_home(&self) -> Result<HomeLayout> {
		let html = Request::get(BASE_URL)?
			.header("User-Agent", USER_AGENT)
			.html()?;

		let mut components = Vec::new();

		// Latest updates: .lst-update shows recent chapter additions as chapter URLs
		// e.g. /Comic/Title/Issue-N?id=xxx → strip to /Comic/Title for the manga key
		let updates = html
			.select(".lst-update .item-list:eq(0) .section.group.list > .col > .sub-col-1 > a")
			.map(|els| {
				els.filter_map(|el| {
					let chapter_url = el.attr("abs:href")?;
					// extract manga key: /Comic/Title from /Comic/Title/Issue-N?id=xxx
					let path = chapter_url.strip_prefix(BASE_URL)?;
					let mut segments = path.splitn(4, '/').filter(|s| !s.is_empty());
					let comic = segments.next()?; // "Comic"
					let title_slug = segments.next()?; // "Title-Slug"
					let key = format!("/{}/{}", comic, title_slug);
					let title = el.text().unwrap_or_default();
					Some(Manga {
						key,
						title,
						url: Some(format!("{BASE_URL}/{}/{}", comic, title_slug)),
						..Default::default()
					}.into())
				})
				.collect::<Vec<_>>()
			})
			.unwrap_or_default();
		if !updates.is_empty() {
			components.push(HomeComponent {
				title: Some("Latest Updates".into()),
				value: HomeComponentValue::Scroller {
					entries: updates,
					listing: None,
				},
				..Default::default()
			});
		}

		Ok(HomeLayout { components })
	}
}

impl DeepLinkHandler for ReadComicOnline {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let Some(path) = url.strip_prefix(BASE_URL) else {
			return Ok(None);
		};

		const COMIC_PATH: &str = "/Comic";

		if !path.starts_with(COMIC_PATH) {
			return Ok(None);
		}

		let mut segments = path.split('/').filter(|s| !s.is_empty());

		let first = segments.next();
		let second = segments.next();

		if let (Some(first), Some(second)) = (first, second) {
			let mut key = String::with_capacity(first.len() + second.len() + 2);
			key.push('/');
			key.push_str(first);
			key.push('/');
			key.push_str(second);
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

register_source!(ReadComicOnline, ListingProvider, Home, DeepLinkHandler);
