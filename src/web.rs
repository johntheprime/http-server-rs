use actix_files::{Files, NamedFile};
use actix_multipart::Multipart;
use actix_web::{
    App, Error, HttpRequest, HttpResponse, HttpServer, Responder, get, middleware, post, web,
};
use futures::StreamExt;

use std::{io::Write, path::PathBuf};

pub async fn run(bind_addr: &str, root: &PathBuf) -> std::io::Result<()> {
    let root_ = root.clone();
    let s = HttpServer::new(move || {
        let static_files = Files::new("/", &root_)
            .show_files_listing()
            .redirect_to_slash_directory()
            .files_listing_renderer(crate::directory_listing::directory_listing);

        App::new()
            .app_data(web::Data::new(root_.clone()))
            .wrap(middleware::Logger::default())
            .service(favicon_ico)
            .service(handle_tar)
            .service(upload_handler)
            .service(static_files)
    })
    .bind(bind_addr)?
    .run();

    log::info!("Serving files from {:?}", &root);
    s.await
}

#[get("/{tail:.*}.tar")]
async fn handle_tar(
    req: HttpRequest,
    root: web::Data<PathBuf>,
    tail: web::Path<String>,
) -> impl Responder {
    let relpath = PathBuf::from(tail.trim_end_matches('/'));
    let fullpath = root.join(&relpath).canonicalize().unwrap();

    // if a .tar already exists, just return it as-is
    let mut fullpath_tar = fullpath.clone();
    fullpath_tar.set_extension("tar");
    if fullpath_tar.is_file() {
        return NamedFile::open_async(fullpath_tar)
            .await
            .unwrap()
            .into_response(&req);
    }

    if !(fullpath.is_dir()) {
        return HttpResponse::NotFound().body("Directory not found\n");
    }

    let stream = crate::threaded_archiver::stream_tar_in_thread(fullpath).map(Ok::<_, Error>);
    let response = HttpResponse::Ok()
        .content_type("application/x-tar")
        .streaming(stream);

    response
}

const FAVICON_ICO: &[u8] = include_bytes!("favicon.png");

#[get("/favicon.ico")]
async fn favicon_ico() -> impl Responder {
    HttpResponse::Ok()
        .content_type("image/png")
        .append_header(("Cache-Control", "only-if-cached, max-age=86400"))
        .body(FAVICON_ICO)
}

#[post("/upload")]
async fn upload_handler(mut payload: Multipart, root: web::Data<PathBuf>) -> impl Responder {
    while let Some(Ok(mut field)) = payload.next().await {
        let content_disposition = field.content_disposition();
        let filename = content_disposition.unwrap().get_filename().unwrap();

        let filepath = root.join(&filename);
        let mut file_buffer = web::BytesMut::new();

        // Collect the uploaded file into memory
        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(bytes) => file_buffer.extend_from_slice(&bytes),
                Err(_) => return HttpResponse::InternalServerError().body("Error reading upload"),
            }
        }

        let result = web::block(move || -> std::io::Result<()> {
            let mut f = std::fs::File::create(filepath)?;
            f.write_all(&file_buffer)?;
            Ok(())
        })
        .await;

        if let Err(e) = result {
            log::error!("File write failed: {}", e);
            return HttpResponse::InternalServerError().body("Failed to write file");
        }
    }

    // Redirect back to the listing after upload
    HttpResponse::SeeOther()
        .insert_header(("Location", "."))
        .finish()
}
