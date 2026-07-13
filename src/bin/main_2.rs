use anyhow::{anyhow, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;
use zip::write::FileOptions;
use zip::ZipWriter;

const MANIFEST_URL: &str = "https://www.xljsci.com/LTSCOfficeV2/manifest.xml";
const REGISTRY_KEY: &str = r"SOFTWARE\Microsoft\Office\16.0\Wef\Developer";

fn main() -> Result<()> {
    let manifest_path = download_manifest()?;
    let manifest_content = fs::read_to_string(&manifest_path)?;

    let addin_id = parse_manifest_id(&manifest_content)?;
    let version = parse_manifest_version(&manifest_content).unwrap_or_else(|_| "1.0.0.0".to_string());
    let host = parse_host(&manifest_content)?;

    if host != "Document" {
        return Err(anyhow!("当前程序只支持 Word 加载项，manifest Host 是: {}", host));
    }

    register_addin(&addin_id, &manifest_path)?;

    let sideload_docx = create_word_sideload_docx(&addin_id, &version)?;
    open_file(&sideload_docx)?;

    println!("Word 加载项已添加");
    println!("Manifest: {}", manifest_path.display());
    println!("Sideload 文档: {}", sideload_docx.display());

    Ok(())
}

fn download_manifest() -> Result<PathBuf> {
    let content = reqwest::blocking::get(MANIFEST_URL)?
        .error_for_status()?
        .text()?;

    let mut dir = dirs::data_local_dir().ok_or_else(|| anyhow!("无法获取 LocalAppData 目录"))?;
    dir.push("XljOfficeAddin");
    fs::create_dir_all(&dir)?;

    let manifest_path = dir.join("manifest.xml");
    fs::write(&manifest_path, content)?;

    Ok(manifest_path)
}

fn parse_manifest_id(xml: &str) -> Result<String> {
    parse_first_text(xml, b"Id").ok_or_else(|| anyhow!("manifest 中没有找到 <Id>"))
}

fn parse_manifest_version(xml: &str) -> Result<String> {
    parse_first_text(xml, b"Version").ok_or_else(|| anyhow!("manifest 中没有找到 <Version>"))
}

fn parse_first_text(xml: &str, tag: &[u8]) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut inside_tag = false;

    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Start(e) => {
                if e.local_name().as_ref() == tag {
                    inside_tag = true;
                }
            }
            Event::Text(e) => {
                if inside_tag {
                    return e.unescape().ok().map(|v| v.to_string());
                }
            }
            Event::End(e) => {
                if e.local_name().as_ref() == tag {
                    inside_tag = false;
                }
            }
            Event::Eof => break,
            _ => {}
        }

        buf.clear();
    }

    None
}

fn parse_host(xml: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(e) | Event::Start(e) => {
                if e.local_name().as_ref() == b"Host" {
                    for attr in e.attributes() {
                        let attr = attr?;
                        if attr.key.as_ref() == b"Name" {
                            return Ok(attr.unescape_value()?.to_string());
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }

        buf.clear();
    }

    Err(anyhow!("manifest 中没有找到 <Host Name=\"...\">"))
}

fn register_addin(addin_id: &str, manifest_path: &Path) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(REGISTRY_KEY)?;
    let manifest_path = manifest_path.to_string_lossy().to_string();

    let _ = key.delete_value(&manifest_path);
    key.set_value(addin_id, &manifest_path)?;

    Ok(())
}

fn create_word_sideload_docx(addin_id: &str, version: &str) -> Result<PathBuf> {
    let mut dir = std::env::temp_dir();
    dir.push("XljOfficeAddin");
    fs::create_dir_all(&dir)?;

    let docx_path = dir.join(format!("Word add-in {}.docx", addin_id));
    let bytes = build_word_docx(addin_id, version)?;
    fs::write(&docx_path, bytes)?;

    Ok(docx_path)
}

fn build_word_docx(addin_id: &str, version: &str) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    add_file(&mut zip, options, "[Content_Types].xml", CONTENT_TYPES)?;
    add_file(&mut zip, options, "_rels/.rels", ROOT_RELS)?;
    add_file(&mut zip, options, "docProps/app.xml", APP_XML)?;
    add_file(&mut zip, options, "word/document.xml", DOCUMENT_XML)?;
    add_file(&mut zip, options, "word/_rels/document.xml.rels", DOCUMENT_RELS)?;
    add_file(&mut zip, options, "word/settings.xml", SETTINGS_XML)?;
    add_file(&mut zip, options, "word/styles.xml", STYLES_XML)?;
    add_file(&mut zip, options, "word/webSettings.xml", WEB_SETTINGS_XML)?;
    add_file(&mut zip, options, "word/fontTable.xml", FONT_TABLE_XML)?;
    add_file(&mut zip, options, "word/webextensions/taskpanes.xml", TASKPANES_XML)?;
    add_file(&mut zip, options, "word/webextensions/_rels/taskpanes.xml.rels", TASKPANES_RELS)?;

    let webextension_xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><we:webextension xmlns:we="http://schemas.microsoft.com/office/webextensions/webextension/2010/11" id="{{{addin_id}}}"><we:reference id="{addin_id}" version="{version}" store="developer" storeType="Registry" /><we:alternateReferences /><we:properties></we:properties><we:bindings /></we:webextension>"#
    );
    add_file(&mut zip, options, "word/webextensions/webextension.xml", &webextension_xml)?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

fn add_file<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    options: FileOptions,
    name: &str,
    content: &str,
) -> Result<()> {
    zip.start_file(name, options)?;
    zip.write_all(content.as_bytes())?;
    Ok(())
}

fn open_file(path: &Path) -> Result<()> {
    Command::new("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .spawn()?;

    Ok(())
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/webSettings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/><Override PartName="/word/webextensions/taskpanes.xml" ContentType="application/vnd.ms-office.webextensiontaskpanes+xml"/><Override PartName="/word/webextensions/webextension.xml" ContentType="application/vnd.ms-office.webextension+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/><Relationship Id="rId3" Type="http://schemas.microsoft.com/office/2011/relationships/webextensiontaskpanes" Target="/word/webextensions/taskpanes.xml"/></Relationships>"#;

const APP_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>Microsoft Office Word</Application></Properties>"#;

const DOCUMENT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:r><w:t>小绿鲸 Word 加载项启动文档</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800" w:header="851" w:footer="992" w:gutter="0"/></w:sectPr></w:body></w:document>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#;

const SETTINGS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:zoom w:percent="100"/></w:settings>"#;

const STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#;

const WEB_SETTINGS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:webSettings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;

const FONT_TABLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:fonts xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;

const TASKPANES_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<wetp:taskpanes xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wetp="http://schemas.microsoft.com/office/webextensions/taskpanes/2010/11"><wetp:taskpane dockstate="" visibility="1" width="350" row="1"><wetp:webextensionref xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="Rc731dad7964e4b0c" /></wetp:taskpane></wetp:taskpanes>"#;

const TASKPANES_RELS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Type="http://schemas.microsoft.com/office/2011/relationships/webextension" Target="/word/webextensions/webextension.xml" Id="Rc731dad7964e4b0c" /></Relationships>"#;
