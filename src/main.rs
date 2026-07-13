use anyhow::{anyhow, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

const MANIFEST_URL: &str = "https://www.xljsci.com/LTSCOfficeV2/manifest.xml";
const SHARE_NAME: &str = "XljOfficeAddinCatalog";
const CATALOG_GUID: &str = "{a81ffdb3-0c25-496c-bfb3-26939675962b}";
const TRUSTED_CATALOGS_KEY: &str = r"SOFTWARE\Microsoft\Office\16.0\WEF\TrustedCatalogs";
const DEVELOPER_KEY: &str = r"SOFTWARE\Microsoft\Office\16.0\Wef\Developer";

fn main() -> Result<()> {
    if !is_elevated() {
        relaunch_as_admin()?;
        println!("需要管理员权限创建 Windows 共享目录，已弹出管理员授权窗口。请在新窗口中继续。");
        return Ok(());
    }

    let (manifest_path, catalog_url) = prepare_catalog()?;
    let manifest_content = fs::read_to_string(&manifest_path)?;

    let addin_id = parse_manifest_id(&manifest_content)?;
    let host = parse_host(&manifest_content)?;

    if host != "Document" {
        return Err(anyhow!("当前程序只支持 Word 加载项，manifest Host 是: {}", host));
    }

    register_trusted_catalog(&catalog_url)?;
    remove_developer_sideload(&addin_id).ok();
    launch_word()?;

    println!("小绿鲸 Word 加载项目录已添加");
    println!("Manifest: {}", manifest_path.display());
    println!("Catalog: {}", catalog_url);
    println!("请完全关闭 Word 后重新打开，再到：插入 -> 加载项 -> 共享文件夹 中查看。首次仍需点添加一次。 ");

    Ok(())
}

fn is_elevated() -> bool {
    let output = Command::new("net").arg("session").output();
    output.map(|o| o.status.success()).unwrap_or(false)
}

fn relaunch_as_admin() -> Result<()> {
    let exe = std::env::current_exe()?;
    let exe = exe.to_string_lossy().replace('"', "`\"");
    let command = format!("Start-Process -FilePath \"{}\" -Verb RunAs", exe);

    Command::new("powershell")
        .args(["-NoProfile", "-Command", &command])
        .spawn()?;

    Ok(())
}
fn prepare_catalog() -> Result<(PathBuf, String)> {
    let content = reqwest::blocking::get(MANIFEST_URL)?
        .error_for_status()?
        .text()?;

    let dir = PathBuf::from(r"C:\Users\Public\XljOfficeAddinCatalog");
    fs::create_dir_all(&dir)?;

    let manifest_path = dir.join("manifest.xml");
    fs::write(&manifest_path, content)?;

    let catalog_url = create_windows_share(&dir)?;
    Ok((manifest_path, catalog_url))
}

fn parse_manifest_id(xml: &str) -> Result<String> {
    parse_first_text(xml, b"Id").ok_or_else(|| anyhow!("manifest 中没有找到 <Id>"))
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

fn create_windows_share(dir: &Path) -> Result<String> {
    let dir = dir.canonicalize()?;
    let share_arg = format!("{}={}", SHARE_NAME, dir.display());

    let _ = Command::new("net")
        .args(["share", SHARE_NAME, "/delete", "/y"])
        .output();

    let output = Command::new("net")
        .args(["share", &share_arg, "/GRANT:Everyone,READ"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "创建 Windows 共享目录失败，请用管理员权限运行此 exe。\n目录: {}\n错误: {}{}",
            dir.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let computer_name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "localhost".to_string());
    Ok(format!(r"\\{}\{}", computer_name, SHARE_NAME))
}

fn register_trusted_catalog(catalog_url: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (root, _) = hkcu.create_subkey(TRUSTED_CATALOGS_KEY)?;
    root.set_value("ClearInstalledExtensions", &0u32).ok();

    let key_path = format!(r"{}\{}", TRUSTED_CATALOGS_KEY, CATALOG_GUID);
    let (key, _) = hkcu.create_subkey(key_path)?;

    key.set_value("Id", &CATALOG_GUID)?;
    key.set_value("Url", &catalog_url)?;
    key.set_value("Flags", &1u32)?;

    Ok(())
}

fn remove_developer_sideload(addin_id: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey_with_flags(DEVELOPER_KEY, KEY_WRITE)?;
    key.delete_value(addin_id).ok();
    Ok(())
}

fn launch_word() -> Result<()> {
    Command::new("cmd")
        .args(["/C", "start", "", "winword"])
        .spawn()?;

    Ok(())
}
