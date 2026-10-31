use std::fs::File;
use std::io::Read;
use std::path::Path;

use percent_encoding::percent_decode_str;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use zip::ZipArchive;

#[derive(Default)]
pub struct Meta {
    pub title: Option<String>,
    pub author: Option<String>,
    pub cover: Option<String>,
}

// "COMP27112_2024.pdf" -> "COMP27112 2024", "attention-is-all.pdf" -> "attention is all"
pub fn title_from_filename(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    // dashes only separate words in names without spaces, "Lamport - Time" keeps its dash
    let kebab = !stem.contains(' ');

    stem.split(|c: char| c == '_' || (kebab && c == '-') || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn epub(path: &Path) -> Meta {
    read_epub(path).unwrap_or_default()
}

pub fn read_zip_entry(path: &Path, name: &str) -> Option<Vec<u8>> {
    let mut archive = ZipArchive::new(File::open(path).ok()?).ok()?;
    let mut entry = archive.by_name(name).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn read_epub(path: &Path) -> Option<Meta> {
    let mut archive = ZipArchive::new(File::open(path).ok()?).ok()?;

    let container = read_text(&mut archive, "META-INF/container.xml")?;
    let opf_path = rootfile(&container)?;
    let opf = read_text(&mut archive, &opf_path)?;

    let opf_dir = opf_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let mut meta = parse_opf(&opf, opf_dir);

    // only keep a cover we can actually serve
    if let Some(cover) = &meta.cover {
        if archive.by_name(cover).is_err() {
            meta.cover = None;
        }
    }

    Some(meta)
}

fn read_text(archive: &mut ZipArchive<File>, name: &str) -> Option<String> {
    let mut entry = archive.by_name(name).ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    Some(text)
}

fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .and_then(|a| a.unescape_value().ok())
        .map(|v| v.into_owned())
}

fn rootfile(container: &str) -> Option<String> {
    let mut reader = Reader::from_str(container);

    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == b"rootfile" => {
                return attr(&e, b"full-path");
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

struct Item {
    id: String,
    href: String,
    media_type: String,
    properties: String,
}

fn parse_opf(opf: &str, opf_dir: &str) -> Meta {
    let mut reader = Reader::from_str(opf);

    let mut title: Option<String> = None;
    let mut creators: Vec<String> = Vec::new();
    let mut cover_id: Option<String> = None;
    let mut items: Vec<Item> = Vec::new();

    // text of the dc:title / dc:creator element we are currently inside
    let mut current: Option<(&[u8], String)> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                manifest(&e, &mut cover_id, &mut items);
                match e.local_name().as_ref() {
                    b"title" if title.is_none() => current = Some((b"title", String::new())),
                    b"creator" => current = Some((b"creator", String::new())),
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => manifest(&e, &mut cover_id, &mut items),
            Ok(Event::Text(t)) => {
                if let Some((_, text)) = current.as_mut() {
                    // badly escaped text (a bare "&") is common enough to keep as-is
                    match t.unescape() {
                        Ok(s) => text.push_str(&s),
                        Err(_) => text.push_str(&String::from_utf8_lossy(&t)),
                    }
                }
            }
            Ok(Event::CData(c)) => {
                if let Some((_, text)) = current.as_mut() {
                    text.push_str(&String::from_utf8_lossy(&c));
                }
            }
            Ok(Event::End(e)) => {
                if let Some((name, text)) = current.take() {
                    if e.local_name().as_ref() != name {
                        current = Some((name, text));
                        continue;
                    }
                    let text = clean(&text);
                    if text.is_empty() {
                        continue;
                    }
                    match name {
                        b"title" => title = Some(text),
                        _ => creators.push(text),
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    let is_image = |i: &&Item| i.media_type.starts_with("image/");

    // epub3 cover-image property, then the epub2 meta, then anything that looks like a cover
    let cover = items
        .iter()
        .find(|i| i.properties.split_whitespace().any(|p| p == "cover-image"))
        .or_else(|| {
            let id = cover_id.as_deref()?;
            items.iter().filter(is_image).find(|i| i.id == id)
        })
        .or_else(|| {
            items
                .iter()
                .filter(is_image)
                .find(|i| i.id.to_lowercase().contains("cover") || i.href.to_lowercase().contains("cover"))
        })
        .map(|i| resolve(opf_dir, &i.href));

    Meta {
        title,
        author: (!creators.is_empty()).then(|| creators.join(", ")),
        cover,
    }
}

// <item> entries and the epub2 <meta name="cover" content="cover-image-id"/>
fn manifest(e: &BytesStart, cover_id: &mut Option<String>, items: &mut Vec<Item>) {
    match e.local_name().as_ref() {
        b"meta" if attr(e, b"name").as_deref() == Some("cover") => *cover_id = attr(e, b"content"),
        b"item" => items.push(Item {
            id: attr(e, b"id").unwrap_or_default(),
            href: attr(e, b"href").unwrap_or_default(),
            media_type: attr(e, b"media-type").unwrap_or_default(),
            properties: attr(e, b"properties").unwrap_or_default(),
        }),
        _ => {}
    }
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// resolve an opf-relative href to a zip entry name
fn resolve(base_dir: &str, href: &str) -> String {
    let href = percent_decode_str(href).decode_utf8_lossy();
    let mut parts: Vec<&str> = base_dir.split('/').filter(|p| !p.is_empty()).collect();

    for segment in href.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }

    parts.join("/")
}
