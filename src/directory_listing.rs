use actix_files::Directory;
use actix_web::dev::ServiceResponse;
use actix_web::{HttpRequest, HttpResponse};
use percent_encoding::{CONTROLS, percent_decode_str, utf8_percent_encode}; // NON_ALPHANUMERIC
use std::fmt::Write;
use std::path::Path;
use v_htmlescape::escape as escape_html_entity;

macro_rules! encode_file_url {
    ($path:ident) => {
        utf8_percent_encode(&$path, CONTROLS)
    };
}

// " -- &quot;  & -- &amp;  ' -- &#x27;  < -- &lt;  > -- &gt;  / -- &#x2f;
macro_rules! encode_file_name {
    ($entry:ident) => {
        escape_html_entity(&$entry.file_name().to_string_lossy())
    };
}

fn format_size(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }

    let mut parts = Vec::new();
    while n > 0 {
        parts.push(format!("{:03}", n % 1000));
        n /= 1000;
    }

    let mut result = parts.into_iter().rev().collect::<Vec<_>>();
    // Remove leading zeros from first group
    if let Some(first) = result.first_mut() {
        *first = first.trim_start_matches('0').to_string();
        if first.is_empty() {
            *first = "0".to_string();
        }
    }
    result.join(",")
}

pub fn directory_listing(
    dir: &Directory,
    req: &HttpRequest,
) -> Result<ServiceResponse, std::io::Error> {
    let encoded_path = req.path().trim_end_matches('/');
    let decoded_path = percent_decode_str(encoded_path)
        .decode_utf8()
        .unwrap_or_else(|_| encoded_path.into());
    let index_of: &str = &decoded_path;
    let mut body = String::new();
    let base = Path::new(req.path());

    for entry in dir.path.read_dir()? {
        if dir.is_visible(&entry) {
            let entry = entry.unwrap();
            let p = match entry.path().strip_prefix(&dir.path) {
                Ok(p) if cfg!(windows) => base.join(p).to_string_lossy().replace('\\', "/"),
                Ok(p) => base.join(p).to_string_lossy().into_owned(),
                Err(_) => continue,
            };

            // if file is a directory, add '/' to the end of the name
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    let _ = write!(
                        body,
                        "<tr><td>📂 <a href='{}/'>{}/</a></td> <td><small>[<a href='{}.tar'>.tar</a>]</small></td></tr>",
                        encode_file_url!(p),
                        encode_file_name!(entry),
                        encode_file_url!(p),
                    );
                } else {
                    let _ = write!(
                        body,
                        "<tr><td>🗎 <a href='{}'>{}</a></td> <td>{} Kb</td></tr>",
                        encode_file_url!(p),
                        encode_file_name!(entry),
                        format_size(metadata.len() / 1024),
                    );
                }
            } else {
                continue;
            }
        }
    }

    let header = format!(
        r#"<h1>Current Dir {} </h1>
         <div class="dir-download">
            <a href="{}.tar" class="download-btn">⬇️ Download .tar</a>
         </div>
         <form method='POST' action='/upload' enctype='multipart/form-data' style='margin-top:1em;'>
         <input type='file' name='file' multiple>
         <input type='submit' value='Upload'>
        </form>"#,
        index_of,
        if index_of.is_empty() { "_" } else { index_of }
    );

    let footer = format!(
        r#"<footer><a href="{}">{} {}</a></footer>"#,
        env!("CARGO_PKG_HOMEPAGE"),
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let style = include_str!("style.css");

    let html = format!(
        r#"<!DOCTYPE html>
         <html>
         <head>
         <meta charset="utf-8" />
         <title>{}</title>
         <style>{}</style></head>
         <body>{}
         <table>
         <tr>
         <td>📁 <a href='../'>../</a></td>
         <td>Size</td>
         </tr>
         {}
         </table>
         {}
         </body></html>"#,
        index_of, style, header, body, footer
    );

    Ok(ServiceResponse::new(
        req.clone(),
        HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html),
    ))
}
