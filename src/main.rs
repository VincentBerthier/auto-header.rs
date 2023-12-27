// File: src/main.rs
// Project: auto-header.rs
// Creation date: mer. 16 août 2023 23:11:03
// Author: Vincent Berthier <vincent.berthier@posteo.org>
// -----
// Last Modified: dim. 20 août 2023 21:14:31
// Modified By: Vincent Berthier
// -----
// Copyright © 2023 <Vincent Berthier> - All rights reserved
#![allow(dead_code)]

use chrono::{DateTime, Local};
use clap::Parser;
use detect_lang::from_path;
use serde::Deserialize;
use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    str,
};

/// Global configuration for the auto-header.
#[derive(Debug, Deserialize)]
struct Config {
    /// Controls wether or not a header should be created if absent.
    #[serde(default)]
    create: bool,
    /// Controls wether or not an existing header should be updated.
    #[serde(default)]
    update: bool,
    /// Determines if we should use the default template for any language
    /// with no specific template defined.
    #[serde(default)]
    language_strict: bool,
    /// Default locale to use for date formatting
    #[serde(default = "default_locale")]
    locale: String,
    /// Data used to fill the templates (names, mail addresses, *etc.*).
    data: ConfigData,
    /// Default template (fall back if no language specific one exists).
    /// It will also serve to fill in the blanks left in language specific
    /// templates.
    default: Template,
    /// Language specific templates.
    language: Option<Vec<Template>>,
    /// Projects configurations.
    project: Option<Vec<Project>>,
}

fn default_locale() -> String {
    String::from("en")
}

/// Data used to fill the templates.
#[derive(Clone, Debug, Deserialize)]
struct ConfigData {
    /// Name of the author.
    author: Option<String>,
    /// Mail address of the author.
    author_mail: Option<String>,
    /// Copyright holders if any.
    cp_holders: Option<String>,
}

impl ConfigData {
    /// Merge a given `ConfigData` with the default one.
    ///
    /// # Arguments
    /// * `default` - `ConfigData` by default, used to fill
    /// unspecified values.
    ///
    /// # Example
    /// ```
    /// let global_config = toml::from_str(fs::read_to_string(args.config)?.as_str())?;
    /// let mut project = find_project(&global_config, "./src/main.rs").unwrap();
    /// project.data = if let Some(data) = project.data {
    ///     Some(data.merge(&config.data))
    /// } else {
    ///     Some(config.data.clone())
    /// };
    /// ```
    fn merge(self, default: &ConfigData) -> Self {
        Self {
            author: Some(self.author.unwrap_or(default.author.clone().unwrap())),
            author_mail: Some(
                self.author_mail
                    .unwrap_or(default.author_mail.clone().unwrap()),
            ),
            cp_holders: Some(
                self.cp_holders
                    .unwrap_or(default.cp_holders.clone().unwrap()),
            ),
        }
    }
}

/// Header template, global or language specific.
#[derive(Clone, Debug, Deserialize)]
struct Template {
    /// Language for which the template applies ("*" for default).
    name: String,
    /// String put at the beginning of every line in the header.
    prefix: Option<String>,
    /// Strings added before the header (such as shebangs for example).
    before: Option<Vec<String>>,
    /// Strings added after the header.
    after: Option<Vec<String>>,
    /// Value of the header template.
    template: Option<String>,
    /// Copyright notice (can be custom or a known license).
    copyright_notice: Option<String>,
    /// Lines that should be updated when an existing header is updated.
    track_changes: Option<Vec<String>>,
}

impl Template {
    /// Merge the current template with the one by default.
    ///
    /// # Arguments
    /// * `default` - `Template` by default, which will be used to fill any
    /// missing values in the language specific template.
    ///
    /// # Example
    /// ```
    /// let global_config = toml::from_str(fs::read_to_string(args.config)?.as_str())?;
    /// let language = String::from("rs");
    /// let language_config = get_language_config(&global_config, &language);
    /// let language_config = language_config.merge(&global_config.default);
    /// ```
    fn merge(self, default: &Template) -> Self {
        Self {
            name: self.name,
            prefix: Some(self.prefix.unwrap_or(default.prefix.clone().unwrap())),
            before: Some(self.before.unwrap_or(default.before.clone().unwrap())),
            after: Some(self.after.clone().unwrap_or(default.after.clone().unwrap())),
            template: Some(self.template.unwrap_or(default.template.clone().unwrap())),
            copyright_notice: Some(
                self.copyright_notice
                    .unwrap_or(default.copyright_notice.clone().unwrap()),
            ),
            track_changes: Some(
                self.track_changes
                    .unwrap_or(default.track_changes.clone().unwrap()),
            ),
        }
    }
}

/// Project configuration.
#[derive(Clone, Debug, Deserialize)]
struct Project {
    /// Root path of the project.
    root: String,
    /// Name of the project.
    name: Option<String>,
    /// Controls wether or not an existing header should be updated for this project.
    create: Option<bool>,
    /// Controls wether or not an existing header should be updated for this project.
    update: Option<bool>,
    /// Locale to format the date with on this project.
    locale: Option<String>,
    /// Data specific to this project.
    data: Option<ConfigData>,
}

/// Application command line’s arguments.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path of the file to update
    #[arg(short, long)]
    path: String,
    #[arg(short, long, default_value_t = format!("{}/auto-header/configuration.toml", env::var("XDG_CONFIG_HOME").unwrap()))]
    config: String,
    #[arg(short, long, default_value_t = false)]
    update_only: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    if !Path::new(&args.path).exists() {
        println!("File {} does not exist.", args.path);
        return Ok(());
    }
    if !Path::new(&args.config).exists() {
        println!("Configuration file {} does not exist.", args.config);
        return Ok(());
    }
    let config: Config = match toml::from_str(fs::read_to_string(args.config)?.as_str()) {
        Ok(config) => config,
        Err(err) => {
            println!("Error reading configuration file: {}", err);
            return Ok(());
        }
    };

    // Get the project’s configuration and check that we’re doing something with it.
    let project = find_project(&config, &args.path);
    let mut project = if let Some(project) = project {
        if !project.create.unwrap_or(config.create) && !project.update.unwrap_or(config.update) {
            println!("Project’s configuration forbids creation and update of headers: exiting.");
            return Ok(());
        }
        project
    } else {
        println!("No configuration found for file {}. Exiting.", args.path);
        return Ok(());
    };
    project.data = Some(if let Some(data) = project.data {
        data.merge(&config.data)
    } else {
        config.data.clone()
    });

    // Get the language for the target file.
    let language = get_language(&args.path);
    let lang_conf = match get_language_config(&config, &language) {
        Some(lang_conf) => lang_conf.merge(&config.default),
        None => {
            println!(
                "No configuration found for file {} (language {}). Exiting.",
                args.path, language
            );
            return Ok(());
        }
    };
    // Build the header.
    let header = fill_template(&lang_conf, &project, &args.path, &project.root);
    // Check if it’s an update or creation, and update / adds the header in the file.
    let header_present = check_header_exists(&args.path, &header, &lang_conf);
    if header_present && config.update {
        match update_header(&args.path, &header, &lang_conf) {
            Ok(_) => (),
            Err(err) => println!("Failed to update header: {}", err),
        }
    } else if !header_present && config.create {
        match write_header(&args.path, &header) {
            Ok(_) => (),
            Err(err) => println!("Failed to write header: {}", err),
        }
    } else {
        println!(
            "nothing to do: header exists = {} with configuration create = {} and update = {}",
            header_present, config.create, config.update
        );
    }
    Ok(())
}

/// Get the file’s language from the extension.
///
/// # Arguments
/// * `path` - path to the file to format.
///
/// # Example
/// ```
/// let lang = get_language("./src/main.rs");
/// ```
fn get_language(path: &str) -> String {
    String::from(match from_path(path) {
        Some(lang) => lang.id(),
        None => "*",
    })
}

/// Get the language specific configuration.
///
/// # Arguments
/// * `config` - Global configuration.
/// * `language` - Language for which we want the configuration.
///
/// # Example
/// ```
/// let config: Config = toml::from_str(fs::read_to_string(args.config)?.as_str())?;
/// let language = get_language(&args.path);
/// let lang_conf = get_language_config(&config, &language);
/// ```
fn get_language_config(config: &Config, language: &str) -> Option<Template> {
    if config.language.is_none() {
        return Some(config.default.clone());
    };
    let res = config
        .language
        .as_ref()
        .unwrap()
        .iter()
        .find(|t| t.name == language);
    match res {
        Some(res) => Some(res.clone()),
        None => {
            if !config.language_strict {
                Some(config.default.clone())
            } else {
                None
            }
        }
    }
}

/// Given the path of the considered file, gets the project’s configuration
/// if it exists.
///
/// # Arguments
/// * `config` - Global configuration.
/// * `path` - Path to the file for which to create or update the header.
///
/// # Example
/// ```
/// let config: Config = toml::from_str(fs::read_to_string(args.config)?.as_str())?;
/// let project = find_project(&config, "./src/main.rs");
/// ```
fn find_project(config: &Config, path: &str) -> Option<Project> {
    if config.project.as_ref().unwrap_or(&Vec::new()).is_empty() {
        return None;
    }
    let path = Path::new(&env::current_dir().unwrap()).join(path);
    let mut path = Some(path.as_path());
    while path.is_some() {
        let project = config
            .project
            .as_ref()
            .unwrap()
            .iter()
            .find(|p| Path::new(&p.root) == path.unwrap());
        match project {
            None => path = path.unwrap().parent(),
            Some(res) => return Some(res.clone()),
        };
    }
    None
}

/// Fills a template with generated or configured data.
///
/// # Arguments
/// * `template` - Template to fill, resulting from the merge of global and language templates.
/// * `project` - Information on the project the file belongs to.
/// * `path` - Path of the file.
/// * `root` - Path to the root of the project the file belongs to.
///
/// # Example
/// ```
/// # let args = Args::parse();
/// # let config = toml::from_str(fs::read_to_string(args.config)?.as_str()).unwrap();
/// let project = find_project(&config, &args.path).unwrap().merge(&config.data);
/// let lang_conf = match get_language_config(&config, &language).unwrap().merge(&config.default);
/// let header = fill_template(&lang_conf, &project, &args.path, &project.root);
/// ```
fn fill_template(template: &Template, project: &Project, path: &str, root: &str) -> Vec<String> {
    let path = Path::new(&env::current_dir().unwrap()).join(path);
    let path = path.strip_prefix(root).unwrap();
    let creation_date: DateTime<Local> = fs::metadata(path.clone())
        .unwrap()
        .created()
        .unwrap()
        .into();
    let creation_date = creation_date.format("%A %d %B %Y").to_string();
    let modification_date: DateTime<Local> = fs::metadata(path.clone())
        .unwrap()
        .modified()
        .unwrap()
        .into();
    let modification_date = modification_date
        .format("%A %d %B %Y @ %H:%M:%S")
        .to_string();
    let year = Local::now().format("%Y").to_string();
    let data = project.data.clone().unwrap();

    let mut res = template
        .template
        .clone()
        .unwrap_or(String::new())
        .as_str()
        .replace(
            "#copyright_notice",
            &template.copyright_notice.clone().unwrap(),
        )
        .to_string();

    res = res
        .replace("#file_creation", &creation_date)
        .replace("#date_now", &modification_date)
        .replace("#file_relative_path", path.to_str().unwrap_or(""))
        .replace(
            "#project_name",
            &project.name.clone().unwrap_or(String::from(
                Path::new(&project.root)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap(),
            )),
        )
        .replace("#author_name", &data.author.unwrap_or(String::new()))
        .replace("#cp_year", &year);
    if data.author_mail.as_ref().is_some_and(|f| !f.is_empty()) {
        res = res.replace(
            "#author_mail",
            format!("<{}>", &data.author_mail.unwrap()).as_str(),
        );
    } else {
        res = res.replace("#author_mail", "");
    }
    if data.cp_holders.as_ref().is_some_and(|f| !f.is_empty()) {
        res = res.replace(
            "#cp_holders",
            format!("<{}>", &data.cp_holders.unwrap()).as_str(),
        );
    } else {
        res = res.replace("#cp_holders", "");
    }

    let prefix = template.prefix.clone().unwrap_or(String::new());
    template
        .before
        .clone()
        .unwrap_or(Vec::new())
        .into_iter()
        .chain(res.split('\n').map(|s| format!("{}{}", prefix, s)))
        .chain(template.after.clone().unwrap_or(Vec::new()))
        .collect()
}

/// Check if a matching header is found in the given file.
///
/// # Arguments
/// * `path` - Path to the file.
/// * `header` - Header generated.
/// * `template` - Template the header was generated from.
///
/// # Example
/// ```
/// # let args = Args::parse();
/// # let config = toml::from_str(fs::read_to_string(args.config)?.as_str()).unwrap();
/// # let project = find_project(&config, &args.path).unwrap().merge(&config.data);
/// # let lang_conf = match get_language_config(&config, &language).unwrap().merge(&config.default);
/// let header = fill_template(&lang_conf, &project, &args.path, &project.root);
/// let exists = check_header_exists(&args.path, &header, &lang_conf);
/// ```
fn check_header_exists(path: &str, header: &[String], template: &Template) -> bool {
    let mut file = File::open(path).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    let content: Vec<String> = content.split('\n').map(|s| s.to_owned()).collect();
    if content.len() < header.len() {
        return false;
    }
    let prefix = template.prefix.clone().unwrap_or(String::new());
    let tracked = template.track_changes.clone().unwrap_or(Vec::new());
    for (hi, ci) in content.iter().zip(header.iter()) {
        if hi != ci
            && !ci.contains("Creation date")
            && !tracked
                .iter()
                .any(|t| ci.replace(&prefix, "").starts_with(t.as_str()))
        {
            return false;
        }
    }
    true
}

/// Updates the fields specified in the track_changes field of the templates for an
/// existing header.
///
/// # Arguments
/// * `path` - Path to the file.
/// * `header` - New generated header.
/// * `template` - Template the header was generated with.
///
/// # Example
/// ```
/// # let args = Args::parse();
/// # let config = toml::from_str(fs::read_to_string(args.config)?.as_str()).unwrap();
/// # let project = find_project(&config, &args.path).unwrap().merge(&config.data);
/// # let lang_conf = match get_language_config(&config, &language).unwrap().merge(&config.default);
/// let header = fill_template(&lang_conf, &project, &args.path, &project.root);
/// let _ = update_header(&args.path, &header, &lang_conf);
///    
/// ```
fn update_header(path: &str, header: &[String], template: &Template) -> Result<(), Box<dyn Error>> {
    let mut f = File::open(path)?;
    let mut content: Vec<u8> = Vec::new();
    f.read_to_end(&mut content)?;
    let content: String = str::from_utf8(&content)?.to_string();
    let mut content: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
    let tracked = template.track_changes.clone().unwrap_or(Vec::new());
    let prefix = template.prefix.clone().unwrap_or(String::new());
    header.iter().enumerate().for_each(|(i, h)| {
        if tracked
            .iter()
            .any(|s| h.replace(&prefix, "").starts_with(s.as_str()))
        {
            content[i] = h.to_string();
        }
    });
    let mut f = File::create(path)?;
    f.write_all(content.join("\n").as_bytes())?;
    Ok(())
}

/// Writes a new header to the file.
///
/// # Arguments
/// * `path` - Path to the file.
/// * `header` - New generated header.
///
/// # Example
/// ```
/// # let args = Args::parse();
/// # let config = toml::from_str(fs::read_to_string(args.config)?.as_str()).unwrap();
/// # let project = find_project(&config, &args.path).unwrap().merge(&config.data);
/// # let lang_conf = match get_language_config(&config, &language).unwrap().merge(&config.default);
/// let header = fill_template(&lang_conf, &project, &args.path, &project.root);
/// let _ = write_header(&args.path, &header, &lang_conf);
///    
/// ```
fn write_header(path: &str, header: &[String]) -> Result<(), Box<dyn Error>> {
    let header = header.join("\n") + "\n";
    let mut f = File::open(path)?;
    let mut content = header.as_bytes().to_owned();
    f.read_to_end(&mut content)?;
    let mut f = File::create(path)?;
    f.write_all(content.as_slice())?;

    Ok(())
}
