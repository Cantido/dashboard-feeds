use std::{fs, path::PathBuf};

use chrono::prelude::*;
use directories::ProjectDirs;
use kdl::{KdlDocument, KdlError};
use miette::{bail, miette, Diagnostic, IntoDiagnostic, NamedSource, Result, SourceSpan};
use owo_colors::OwoColorize;
use reqwest::blocking::Client;
use syndication::Feed;
use textwrap::{fill, Options};
use thiserror::Error;

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

struct FeedItem {
    feed_title: String,
    title: String,
    link: String,
    pub_date: DateTime<FixedOffset>,
}

fn main() -> Result<()> {
    let mut urls: Vec<String> = vec![];

    let project_dirs = ProjectDirs::from("dev", "cosmicrose", "dashboard-feedreader")
        .expect("Standard project dir should be available on the operating system");

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

    let client = Client::new();

    let mut feed_items = vec![];

    for url in urls {
        let res = client
            .get(url)
            .send()
            .into_diagnostic()?
            .text()
            .into_diagnostic()?;

        match res.parse::<Feed>().unwrap() {
            Feed::RSS(rss_feed) => {
                for item in rss_feed.items() {
                    let feed_item = FeedItem {
                        feed_title: rss_feed.title().to_string(),
                        title: item.title().unwrap_or("").to_string(),
                        link: item.link().unwrap_or("").to_string(),
                        pub_date: DateTime::parse_from_rfc2822(item.pub_date().unwrap()).unwrap(),
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
    }

    feed_items.sort_by_key(|f| f.pub_date);
    feed_items.reverse();

    let title_wrap_options = Options::with_termwidth()
        .initial_indent("- ")
        .subsequent_indent("    ");

    for feed_item in feed_items.iter().take(10) {
        let feed_line = format!("{}: {}", feed_item.feed_title.dimmed(), feed_item.title);
        println!("{}", fill(&feed_line, &title_wrap_options));
    }

    Ok(())
}
