use std::{fs, path::PathBuf, time::Duration};

use chrono::prelude::*;
use clap::Parser;
use directories::ProjectDirs;
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use kdl::{KdlDocument, KdlError};
use miette::{bail, miette, Diagnostic, IntoDiagnostic, NamedSource, Result, SourceSpan};
use owo_colors::OwoColorize;
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use supports_hyperlinks::supports_hyperlinks;
use syndication::Feed;
use textwrap::{fill, Options};
use thiserror::Error;
use tokio::task::JoinSet;

static USER_AGENT: &str = concat!(
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    " +",
    env!("CARGO_PKG_HOMEPAGE"),
);

#[derive(Error, Diagnostic, Debug)]
pub enum ApplicationError {}

#[derive(Error, Diagnostic, Debug)]
pub enum ConfigurationError {
    #[error("Could not read configuration file at {}", path.display())]
    CouldNotRead {
        path: PathBuf,

        #[source]
        cause: std::io::Error,
    },

    #[error("Could not parse configuration file at {}", source_code.name())]
    #[diagnostic()]
    CouldNotParse {
        #[source_code]
        source_code: NamedSource<String>,

        #[source]
        #[diagnostic_source]
        cause: KdlError,
    },

    #[error("Configuration node \"feeds\" is missing or doesn't have any entries")]
    #[diagnostic(help(
        "Add feeds like this:

    feeds {{
        url \"https://blog.rust-lang.org/feed.xml\"
    }}"
    ))]
    MissingFeeds {
        #[source_code]
        src: NamedSource<String>,

        #[label("missing from here")]
        span: SourceSpan,
    },

    #[error("Configured list of feeds has a bad entry")]
    #[diagnostic(help(
        "Feed entries should look like this:

    url \"https://blog.rust-lang.org/feed.xml\""
    ))]
    InvalidFeedUrl {
        #[source_code]
        src: NamedSource<String>,

        #[label("this should have a URL string argument")]
        span: SourceSpan,
    },
}

pub trait Hyperlink<S: AsRef<str>, T: AsRef<str>> {
    fn hyperlink(&self, url: T) -> String;
}

impl<S: AsRef<str>, T: AsRef<str>> Hyperlink<S, T> for S {
    fn hyperlink(&self, url: T) -> String {
        format!(
            "\u{1b}]8;;{}\u{1b}\\{}\u{1b}]8;;\u{1b}\\",
            url.as_ref(),
            self.as_ref()
        )
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// How many entries to return
    #[arg(short = 'n', long, default_value_t = 20)]
    limit: usize,
}

#[derive(Clone)]
struct FeedItem {
    feed_title: String,
    title: String,
    link: String,
    pub_date: DateTime<FixedOffset>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let mut urls: Vec<String> = vec![];

    let project_dirs = ProjectDirs::from("dev", "cosmicrose", "dashboard-feeds")
        .expect("Standard project dir should be available on the operating system");

    let cache_path = project_dirs.cache_dir().join("http");
    let config_path = project_dirs.config_dir().join("config.kdl");

    if !config_path.exists() {
        bail!("Config file not found at {}", &config_path.display());
    };

    let config_content =
        fs::read_to_string(&config_path).map_err(|e| ConfigurationError::CouldNotRead {
            path: config_path.clone(),
            cause: e,
        })?;
    let config_doc: KdlDocument =
        config_content
            .parse()
            .map_err(|e: KdlError| ConfigurationError::CouldNotParse {
                source_code: NamedSource::new(
                    config_path.to_str().unwrap(),
                    config_content.clone(),
                ),
                cause: e,
            })?;

    let feeds_node = config_doc
        .get("feeds")
        .ok_or(ConfigurationError::MissingFeeds {
            src: NamedSource::new(config_path.to_str().unwrap(), config_content.clone()),
            span: config_doc.span(),
        })?;

    let feed_nodes = feeds_node
        .children()
        .ok_or(ConfigurationError::MissingFeeds {
            src: NamedSource::new(config_path.to_str().unwrap(), config_content.clone()),
            span: config_doc.span(),
        })?
        .nodes();

    if feed_nodes.is_empty() {
        bail!(ConfigurationError::MissingFeeds {
            src: NamedSource::new(config_path.to_str().unwrap(), config_content.clone()),
            span: config_doc.span(),
        });
    }

    for node in feed_nodes {
        let url = node
            .get(0)
            .ok_or(ConfigurationError::InvalidFeedUrl {
                src: NamedSource::new(config_path.to_str().unwrap(), config_content.clone()),
                span: node.span(),
            })?
            .as_string()
            .ok_or(miette!("Configured feed entry is not a string"))?;

        urls.push(url.to_string());
    }

    let client = ClientBuilder::new(
        Client::builder()
            .brotli(true)
            .gzip(true)
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(10))
            .build()
            .into_diagnostic()?,
    )
    .with(Cache(HttpCache {
        mode: CacheMode::Default,
        manager: CACacheManager { path: cache_path },
        options: HttpCacheOptions::default(),
    }))
    .build();

    let mut join_set: JoinSet<Result<Vec<FeedItem>>> = JoinSet::new();

    for url in urls {
        let task_client = client.clone();

        join_set.spawn(async move {
            let res = task_client
                .get(url)
                .send()
                .await
                .into_diagnostic()?
                .text()
                .await
                .into_diagnostic()?;

            let mut feed_items = vec![];

            match res.parse::<Feed>().unwrap() {
                Feed::RSS(rss_feed) => {
                    for item in rss_feed.items() {
                        let feed_item = FeedItem {
                            feed_title: rss_feed.title().to_string(),
                            title: item.title().unwrap_or("").to_string(),
                            link: item.link().unwrap_or("").to_string(),
                            pub_date: DateTime::parse_from_rfc2822(item.pub_date().unwrap())
                                .unwrap(),
                        };

                        feed_items.push(feed_item);
                    }
                }
                Feed::Atom(atom_feed) => {
                    for item in atom_feed.entries() {
                        let feed_item = FeedItem {
                            feed_title: atom_feed.title().to_string(),
                            title: item.title().to_string(),
                            link: item.links()[0].href().to_string(),
                            pub_date: item.updated().parse().unwrap(),
                        };

                        feed_items.push(feed_item);
                    }
                }
            }

            // Cut down the list of results here while we are in a separate task
            // so that we have less sort when all the tasks are joined.

            feed_items.sort_by_key(|f| f.pub_date);
            feed_items.reverse();

            let chunk: Vec<FeedItem> = feed_items.into_iter().take(args.limit).collect();

            Ok(chunk)
        });
    }

    let mut feed_items = vec![];

    while let Some(task_result) = join_set.join_next().await {
        if let Ok(fetch_result) = task_result {
            if let Ok(feed_batch) = fetch_result {
                feed_items.extend(feed_batch);
            }
        }
    }

    feed_items.sort_by_key(|f| f.pub_date);
    feed_items.reverse();

    let title_wrap_options = Options::with_termwidth()
        .initial_indent("- ")
        .subsequent_indent("    ");

    for feed_item in feed_items.iter().take(args.limit) {
        let feed_line: String = format!("{}: {}", feed_item.feed_title.dimmed(), feed_item.title);
        let feed_text = fill(&feed_line, &title_wrap_options);

        let feed_link = if supports_hyperlinks() {
            feed_text.hyperlink(&feed_item.link)
        } else {
            feed_text
        };
        println!("{}", &feed_link)
    }

    Ok(())
}
