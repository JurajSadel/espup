use crate::chip::Chip;
use crate::emoji;
use crate::InstallOpts;
use anyhow::{bail, Result};
use dirs::home_dir;
use flate2::bufread::GzDecoder;
use log::{debug, info};
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::process::Stdio;
use std::{fs, io};
use tar::Archive;
use xz2::read::XzDecoder;

pub fn parse_targets(build_target: &str) -> Result<Vec<Chip>, String> {
    debug!("{} Parsing targets: {}", emoji::DEBUG, build_target);
    let mut chips: Vec<Chip> = Vec::new();
    if build_target.contains("all") {
        chips.push(Chip::ESP32);
        chips.push(Chip::ESP32S2);
        chips.push(Chip::ESP32S3);
        chips.push(Chip::ESP32C3);
        return Ok(chips);
    }
    let targets: Vec<&str> = if build_target.contains(' ') || build_target.contains(',') {
        build_target.split([',', ' ']).collect()
    } else {
        vec![build_target]
    };
    for target in targets {
        match target {
            "esp32" => chips.push(Chip::ESP32),
            "esp32s2" => chips.push(Chip::ESP32S2),
            "esp32s3" => chips.push(Chip::ESP32S3),
            "esp32c3" => chips.push(Chip::ESP32C3),
            _ => {
                return Err(format!("Unknown target: {}", target));
            }
        };
    }

    Ok(chips)
}

pub fn parse_llvm_version(llvm_version: &str) -> Result<String, String> {
    let parsed_version = match llvm_version {
        "13" => "esp-13.0.0-20211203",
        "14" => "esp-14.0.0-20220415",
        "15" => "", // TODO: Fill when released
        _ => {
            return Err(format!("Unknown LLVM Version: {}", llvm_version));
        }
    };

    Ok(parsed_version.to_string())
}

pub fn get_llvm_version_with_underscores(llvm_version: &str) -> String {
    let version: Vec<&str> = llvm_version.split('-').collect();
    let llvm_dot_version = version[1];
    llvm_dot_version.replace('.', "_")
}

pub fn get_artifact_llvm_extension(arch: &str) -> &str {
    match arch {
        "x86_64-pc-windows-msvc" => "zip",
        "x86_64-pc-windows-gnu" => "zip",
        _ => "tar.xz",
    }
}

pub fn get_llvm_arch(arch: &str) -> &str {
    match arch {
        "aarch64-apple-darwin" => "macos",
        "x86_64-apple-darwin" => "macos",
        "x86_64-unknown-linux-gnu" => "linux-amd64",
        "x86_64-pc-windows-msvc" => "win64",
        "x86_64-pc-windows-gnu" => "win64",
        _ => arch,
    }
}

pub fn get_gcc_artifact_extension(arch: &str) -> &str {
    match arch {
        "x86_64-pc-windows-msvc" => "zip",
        "x86_64-pc-windows-gnu" => "zip",
        _ => "tar.gz",
    }
}

pub fn get_gcc_arch(arch: &str) -> &str {
    match arch {
        "aarch64-apple-darwin" => "macos",
        "aarch64-unknown-linux-gnu" => "linux-arm64",
        "x86_64-apple-darwin" => "macos",
        "x86_64-unknown-linux-gnu" => "linux-amd64",
        "x86_64-pc-windows-msvc" => "win64",
        "x86_64-pc-windows-gnu" => "win64",
        _ => arch,
    }
}

pub fn get_rust_installer(arch: &str) -> &str {
    match arch {
        "x86_64-pc-windows-msvc" => "",
        "x86_64-pc-windows-gnu" => "",
        _ => "./install.sh",
    }
}

pub fn get_home_dir() -> String {
    home_dir().unwrap().display().to_string()
}

pub fn get_tools_path() -> String {
    env::var("IDF_TOOLS_PATH").unwrap_or_else(|_e| get_home_dir() + "/.espressif")
}

pub fn get_tool_path(tool_name: &str) -> String {
    format!("{}/tools/{}", get_tools_path(), tool_name)
}

pub fn get_dist_path(tool_name: &str) -> String {
    let tools_path = get_tools_path();
    format!("{}/dist/{}", tools_path, tool_name)
}

pub fn get_espidf_path(version: &str) -> String {
    let parsed_version: String = version
        .chars()
        .map(|x| match x {
            '/' => '-',
            _ => x,
        })
        .collect();
    format!("{}frameworks/esp-idf-{}", get_tools_path(), parsed_version)
}

pub fn download_file(
    url: String,
    file_name: &str,
    output_directory: &str,
    uncompress: bool,
) -> Result<String> {
    let file_path = format!("{}/{}", output_directory, file_name);
    if Path::new(&file_path).exists() {
        info!("{} Using cached file: {}", emoji::INFO, file_path);
        return Ok(file_path);
    } else if !Path::new(&output_directory).exists() {
        info!("{} Creating directory: {}", emoji::WRENCH, output_directory);
        if let Err(_e) = fs::create_dir_all(output_directory) {
            bail!(
                "{} Creating directory {} failed",
                emoji::ERROR,
                output_directory
            );
        }
    }
    info!(
        "{} Downloading file {} from {}",
        emoji::DOWNLOAD,
        file_name,
        url
    );
    let mut resp = reqwest::blocking::get(&url).unwrap();

    if uncompress {
        let extension = Path::new(file_name).extension().unwrap().to_str().unwrap();
        match extension {
            "zip" => {
                let mut tmpfile = tempfile::tempfile().unwrap();
                resp.copy_to(&mut tmpfile)?;
                let mut zipfile = zip::ZipArchive::new(tmpfile).unwrap();
                zipfile.extract(output_directory).unwrap();
            }
            "gz" => {
                info!(
                    "{} Uncompressing tar.gz file to {}",
                    emoji::WRENCH,
                    output_directory
                );
                let content_br = BufReader::new(resp);
                let tarfile = GzDecoder::new(content_br);
                let mut archive = Archive::new(tarfile);
                archive.unpack(output_directory).unwrap();
            }
            "xz" => {
                info!(
                    "{} Uncompressing tar.xz file to {}",
                    emoji::WRENCH,
                    output_directory
                );
                let content_br = BufReader::new(resp);
                let tarfile = XzDecoder::new(content_br);
                let mut archive = Archive::new(tarfile);
                archive.unpack(output_directory).unwrap();
            }
            _ => {
                bail!("{} Unsuported file extension: {}", emoji::ERROR, extension);
            }
        }
    } else {
        info!("{} Creating file: {}", emoji::WRENCH, file_path);
        let mut out = File::create(file_path)?;
        io::copy(&mut resp, &mut out)?;
    }
    Ok(format!("{}/{}", output_directory, file_name))
}

#[cfg(windows)]
pub fn run_command(
    shell: String,
    arguments: Vec<String>,
    command: String,
) -> std::result::Result<(), clap::Error> {
    debug!("{} Command arguments: {:?}", emoji::DEBUG, arguments);
    let mut child_process = std::process::Command::new(shell)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    {
        let child_stdin = child_process.stdin.as_mut().unwrap();
        child_stdin.write_all(&*command.into_bytes())?;
        // Close stdin to finish and avoid indefinite blocking
        drop(child_stdin);
    }
    let output = child_process.wait_with_output()?;
    debug!("{} Command output = {:?}", emoji::DEBUG, output);
    Ok(())
}

#[cfg(unix)]
pub fn run_command(
    shell: &str,
    arguments: Vec<String>,
    command: String,
) -> std::result::Result<std::process::Output, anyhow::Error> {
    // Unix - pass command as parameter for initializer
    let mut arguments = arguments;
    if !command.is_empty() {
        arguments.push(command);
    }
    debug!("{} Command arguments: {:?}", emoji::DEBUG, arguments);

    let child_process = std::process::Command::new(shell)
        .args(&arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    {}
    let output = child_process.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "{} Command {} with args {:?} failed. Output: {:#?}",
            emoji::ERROR,
            shell,
            arguments,
            output
        );
    }
    Ok(output)
}

// pub fn get_python_env_path(idf_version: &str, python_version: &str) -> String {
//     let tools_path = get_tools_path();
//     format!(
//         "{}/python_env/idf{}_py{}_env",
//         tools_path, idf_version, python_version
//     )
// }

pub fn print_arguments(args: &InstallOpts, arch: &str, targets: &Vec<Chip>, llvm_version: &str) {
    debug!(
        "{} Arguments:
            - Arch: {}
            - Build targets: {:?}
            - Cargo home: {:?}
            - Clear cache: {:?}
            - ESP-IDF version: {:?}
            - Export file: {:?}
            - Extra crates: {:?}
            - LLVM version: {:?}
            - Minified ESP-IDF: {:?}
            - Nightly version: {:?}
            - Rustup home: {:?}
            - Toolchain version: {:?}
            - Toolchain destination: {:?}",
        emoji::INFO,
        arch,
        targets,
        &args.cargo_home,
        args.clear_cache,
        &args.espidf_version,
        &args.export_file,
        args.extra_crates,
        llvm_version,
        &args.minified_espidf,
        args.nightly_version,
        &args.rustup_home,
        args.toolchain_version,
        &args.toolchain_destination
    );
}
