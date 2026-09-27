/*

######################################################
# Here's everything you need to know about exporting files. 
# Specifically, there are two techniques for exporting 
# files: the "classic" one (used for handwritten documents) 
# and the PDF Injection one, which simply writes what we've 
# done to a PDF, rather than redrawing the surface.
######################################################

*/ 

use crate::models::page::*;
use crate::save_handler::db::*;
use crate::save_handler::database_pdf_utilities::*;
use crate::save_handler::database_utilities::*;
use crate::save_handler::autosave::*;
use crate::save_handler::autosave_utilities::*;


use std::path::Path;
use std::sync::mpsc::Sender;

use lopdf::{Document, Object, Dictionary, Stream};
use lopdf::content::{Content, Operation};
use cairo::Context;

pub fn export_native_via_cairo(
    conn: &rusqlite::Connection,
    output_path: &Path,
    count: usize,
    tx: &Sender<Result<Option<(usize, usize)>, String>>,
) -> Result<(), String> {
    let _ = tx.send(Ok(Some((0, count))));

    let surface = cairo::PdfSurface::new(PAGE_W, PAGE_H, output_path)
        .map_err(|e| format!("Impossibile creare il PDF: {e}"))?;

    let image_cache = std::cell::RefCell::new(std::collections::HashMap::new());

    for idx in 0..count {
        let page_id = page_id_at(conn, idx).map_err(|e| e.to_string())?;
        let page_data = load_page(conn, page_id).map_err(|e| e.to_string())?;

        let ctx = cairo::Context::new(&surface).map_err(|e| e.to_string())?;

        
        ctx.set_source_rgb(1.0, 1.0, 1.0);
        let _ = ctx.paint();

        render_components(&ctx, &page_data, &image_cache);

        ctx.show_page().map_err(|e| e.to_string())?;
        let _ = tx.send(Ok(Some((idx + 1, count))));
    }

    surface.finish();
    Ok(())
}

fn export_via_pdf_injection(
    conn: &rusqlite::Connection,
    base_doc_id: i64,
    output_path: &Path,
    count: usize,
    tx: &Sender<Result<Option<(usize, usize)>, String>>,
) -> Result<(), String> {
    let _ = tx.send(Ok(Some((0, count))));

    
    let row = get_pdf_document(conn, base_doc_id).map_err(|e| e.to_string())?;
    let full_path = SESSION_TEMP_DIR.path().join(&row.relative_path);

    
    let mut doc = Document::load(&full_path).map_err(|e| format!("Errore lettura PDF nativo: {e}"))?;
    let pages = doc.get_pages();

    let image_cache = std::cell::RefCell::new(std::collections::HashMap::new());

    
    for idx in 0..count {
        let page_id = page_id_at(conn, idx).map_err(|e| e.to_string())?;
        let page_data = load_page(conn, page_id).map_err(|e| e.to_string())?;

        
        if page_data.components.is_empty() {
            let _ = tx.send(Ok(Some((idx + 1, count))));
            continue;
        }

        let pdf_ref = get_page_pdf_ref(conn, page_id).map_err(|e| e.to_string())?;

        if let Some((doc_id, page_index)) = pdf_ref {
            if doc_id == base_doc_id {
                let pdf_page_num = (page_index + 1) as u32;

                if let Some(&page_obj_id) = pages.get(&pdf_page_num) {

                    
                    let mut width = 595.0;
                    let mut height = 842.0;

                    if let Ok(page_dict) = doc.get_object(page_obj_id).and_then(Object::as_dict) {
                        if let Ok(media_box) = page_dict.get(b"MediaBox").and_then(Object::as_array) {
                            if media_box.len() == 4 {
                                
                                let get_num = |obj: &Object| -> f64 {
                                    match obj {
                                        Object::Integer(i) => *i as f64,
                                        Object::Real(f) => *f as f64,
                                        _ => 0.0,
                                    }
                                };

                                let x1 = get_num(&media_box[0]);
                                let y1 = get_num(&media_box[1]);
                                let x2 = get_num(&media_box[2]);
                                let y2 = get_num(&media_box[3]);

                                width = (x2 - x1).abs();
                                height = (y2 - y1).abs();
                            }
                        }
                    }

                    
                    let render_scale = 3.0; 
                    let target_w = (PAGE_W * render_scale) as i32;
                    let target_h = (PAGE_H * render_scale) as i32;

                    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, target_w, target_h)
                        .map_err(|e| e.to_string())?;

                    let ctx = cairo::Context::new(&surface).map_err(|e| e.to_string())?;
                    ctx.scale(render_scale, render_scale);

                    
                    render_components(&ctx, &page_data, &image_cache);
                    drop(ctx); 

                    
                    let mut rgb = Vec::with_capacity((target_w * target_h * 3) as usize);
                    let mut alpha = Vec::with_capacity((target_w * target_h) as usize);

                    let data = surface.data().unwrap();
                    for chunk in data.chunks_exact(4) {
                        
                        let b = chunk[0] as u32;
                        let g = chunk[1] as u32;
                        let r = chunk[2] as u32;
                        let a = chunk[3] as u32;

                        alpha.push(a as u8);
                        if a == 0 {
                            rgb.push(255); rgb.push(255); rgb.push(255);
                        } else {
                            
                            rgb.push((r * 255 / a) as u8);
                            rgb.push((g * 255 / a) as u8);
                            rgb.push((b * 255 / a) as u8);
                        }
                    }
                    drop(data);

                    
                    
                    let mut smask_dict = Dictionary::new();
                    smask_dict.set("Type", "XObject");
                    smask_dict.set("Subtype", "Image");
                    smask_dict.set("Width", target_w);
                    smask_dict.set("Height", target_h);
                    smask_dict.set("ColorSpace", "DeviceGray");
                    smask_dict.set("BitsPerComponent", 8);
                    let mut smask_stream = Stream::new(smask_dict, alpha);
                    let _ = smask_stream.compress(); 
                    let smask_id = doc.add_object(smask_stream);

                    
                    let mut img_dict = Dictionary::new();
                    img_dict.set("Type", "XObject");
                    img_dict.set("Subtype", "Image");
                    img_dict.set("Width", target_w);
                    img_dict.set("Height", target_h);
                    img_dict.set("ColorSpace", "DeviceRGB");
                    img_dict.set("BitsPerComponent", 8);
                    img_dict.set("SMask", smask_id);
                    let mut img_stream = Stream::new(img_dict, rgb);
                    let _ = img_stream.compress();
                    let img_id = doc.add_object(img_stream);

                    let xobj_name = format!("RastinOverlay{}", idx);
                    add_xobject_to_page(&mut doc, page_obj_id, &xobj_name, img_id)?;

                    
                    let content_ops = vec![
                        Operation::new("q", vec![]),
                        
                        Operation::new("cm", vec![
                            Object::Real(width as f32), Object::Integer(0),
                            Object::Integer(0), Object::Real(-height as f32),
                            Object::Integer(0), Object::Real(height as f32),
                        ]),
                        Operation::new("Do", vec![Object::Name(xobj_name.as_bytes().to_vec())]),
                        Operation::new("Q", vec![]),
                    ];

                    let content = Content { operations: content_ops };
                    let _ = doc.add_to_page_content(page_obj_id, content);
                }
            }
        }
        let _ = tx.send(Ok(Some((idx + 1, count))));
    }

    
    doc.save(output_path).map_err(|e| format!("Impossibile salvare il PDF elaborato: {e}"))?;

    Ok(())
}

pub fn export_document_to_pdf(
    db_path: &Path,
    output_path: &Path,
    tx: Sender<Result<Option<(usize, usize)>, String>>,
) {
    let res = (|| -> Result<(), String> {
        let conn = rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI
        ).map_err(|e| format!("Errore apertura DB: {e}"))?;

        let count = page_count(&conn).map_err(|e| e.to_string())? as usize;
        if count == 0 { return Ok(()); }

        let base_doc_id = (0..count)
            .filter_map(|i| page_id_at(&conn, i).ok())
            .find_map(|pid| get_page_pdf_ref(&conn, pid).ok().flatten())
            .map(|(doc_id, _)| doc_id);

        match base_doc_id {
            Some(doc_id) => export_via_pdf_injection(&conn, doc_id, output_path, count, &tx),
            None => export_native_via_cairo(&conn, output_path, count, &tx),
        }
    })();

    let _ = match res {
        Ok(_) => tx.send(Ok(None)),
        Err(e) => tx.send(Err(e)),
    };
}


fn add_xobject_to_page(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    xobj_name: &str,
    xobj_id: lopdf::ObjectId,
) -> Result<(), String> {
    let page_dict = doc.get_object_mut(page_id).and_then(Object::as_dict_mut)
        .map_err(|_| "Pagina non trovata nell'albero")?;
    
    if !page_dict.has(b"Resources") {
        page_dict.set("Resources", Dictionary::new());
    }
    
    let res_obj = page_dict.get(b"Resources").unwrap().clone();
    match res_obj {
        Object::Dictionary(mut dict) => {
            if !dict.has(b"XObject") {
                dict.set("XObject", Dictionary::new());
            }
            if let Ok(xobj_dict) = dict.get_mut(b"XObject").and_then(Object::as_dict_mut) {
                xobj_dict.set(xobj_name.as_bytes().to_vec(), Object::Reference(xobj_id));
            }
            let page_mut = doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap();
            page_mut.set("Resources", dict);
        }
        Object::Reference(ref_id) => {
            let dict = doc.get_object_mut(ref_id).and_then(Object::as_dict_mut)
                .map_err(|_| "Oggetto Resources non trovato")?;
            if !dict.has(b"XObject") {
                dict.set("XObject", Dictionary::new());
            }
            if let Ok(xobj_dict) = dict.get_mut(b"XObject").and_then(Object::as_dict_mut) {
                xobj_dict.set(xobj_name.as_bytes().to_vec(), Object::Reference(xobj_id));
            }
        }
        _ => return Err("Dizionario Resources non supportato".into()),
    }
    Ok(())
}

fn render_components(
    cr: &Context,
    page: &PageData,
    image_cache: &std::cell::RefCell<std::collections::HashMap<String, cairo::ImageSurface>>,
) {
    for component in &page.components {
        match component {
            ComponentPayload::Image(block) => {
                crate::models::image::render_image_block(cr, block, 0.0, 0.0, image_cache);
            }
            ComponentPayload::PenStroke(stroke) => {
                if stroke.points.len() < 2 { continue; }
                cr.set_operator(cairo::Operator::Over);
                cr.set_source_rgb(stroke.color.r, stroke.color.g, stroke.color.b);
                cr.set_line_width(stroke.width);
                cr.set_line_cap(cairo::LineCap::Round);
                cr.set_line_join(cairo::LineJoin::Round);
                let (sx, sy) = stroke.points[0];
                cr.move_to(sx, sy);
                for &(px, py) in &stroke.points[1..] {
                    cr.line_to(px, py);
                }
                let _ = cr.stroke();
            }
            ComponentPayload::EraserStroke(_) => {}
            ComponentPayload::RichText(block) => {
                crate::models::textbox::render_rich_text_block(cr, block, 0.0, 0.0, image_cache);
            }
            ComponentPayload::Shape(block) => {
                crate::models::page::render_shape(cr, block, 0.0, 0.0);
            }
        }
    }
}