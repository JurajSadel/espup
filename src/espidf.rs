//! GCC Toolchain source and installation tools

use crate::chip::Chip;
use crate::emoji;
use crate::gcc_toolchain::{get_toolchain_name, get_ulp_toolchain_name};
use crate::utils::get_tools_path;
use anyhow::{Context, Result};
use embuild::espidf::EspIdfRemote;
use embuild::{espidf, git};
use log::{debug, info};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use strum::{Display, EnumIter, EnumString, IntoStaticStr};

const DEFAULT_GIT_REPOSITORY: &str = "https://github.com/espressif/esp-idf";

pub const DEFAULT_CMAKE_GENERATOR: Generator = {
    // No Ninja builds for linux=aarch64 from Espressif yet
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        Generator::UnixMakefiles
    }

    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    {
        Generator::Ninja
    }
};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, EnumString, Display, EnumIter, IntoStaticStr)]
pub enum Generator {
    Ninja,
    NinjaMultiConfig,
    UnixMakefiles,
    BorlandMakefiles,
    MSYSMakefiles,
    MinGWMakefiles,
    NMakeMakefiles,
    NMakeMakefilesJOM,
    WatcomWMake,
}
#[derive(Debug)]
pub struct EspIdf {
    /// The repository containing GCC sources.
    pub repository_url: String,
    /// ESP-IDF Version.
    pub version: String,
    /// Minify ESP-IDF?.
    pub minified: bool,
    /// Installation directory.
    pub install_path: PathBuf,
    /// ESP targets.
    pub targets: Vec<Chip>,
}

impl EspIdf {
    /// Installs esp-idf.
    pub fn install(self, minify: bool) -> Result<PathBuf> {
        let cmake_generator = DEFAULT_CMAKE_GENERATOR;

        // A closure to specify which tools `idf-tools.py` should install.
        let make_tools = move |repo: &git::Repository,
                               version: &Result<espidf::EspIdfVersion>|
              -> Result<Vec<espidf::Tools>> {
            info!(
                "{} Using esp-idf {} at '{}'",
                emoji::INFO,
                espidf::EspIdfVersion::format(version),
                repo.worktree().display()
            );

            let mut tools = vec![];
            let mut subtools = Vec::new();
            for target in self.targets.clone() {
                let gcc_toolchain_name = get_toolchain_name(target);
                subtools.push(gcc_toolchain_name);

                let ulp_toolchain_name = get_ulp_toolchain_name(target, version.as_ref().ok());
                if !cfg!(target_os = "linux") || !cfg!(target_arch = "aarch64") {
                    if let Some(ulp_toolchain_name) = ulp_toolchain_name {
                        subtools.push(ulp_toolchain_name);
                    }
                }
            }

            // Use custom cmake for esp-idf<4.4, because we need at least cmake-3.20
            match version.as_ref().map(|v| (v.major, v.minor, v.patch)) {
                Ok((major, minor, _)) if major >= 4 && minor >= 4 => {
                    subtools.push("cmake".to_string())
                }
                _ => {
                    tools.push(espidf::Tools::cmake()?);
                }
            }

            subtools.push("openocd-esp32".to_string());
            #[cfg(windows)]
            subtools.push("idf-exe".to_string());
            #[cfg(windows)]
            subtools.push("ccache".to_string());
            #[cfg(windows)]
            subtools.push("dfu-util".to_string());

            if cmake_generator == Generator::Ninja {
                subtools.push("ninja".to_string())
            }

            tools.push(espidf::Tools::new(subtools));

            Ok(tools)
        };
        let install = |esp_idf_origin: espidf::EspIdfOrigin| -> Result<espidf::EspIdf> {
            espidf::Installer::new(esp_idf_origin)
                .install_dir(Some(self.install_path.clone()))
                .with_tools(make_tools)
                .install()
                .context("Could not install esp-idf")
        };

        let repo = espidf::EspIdfRemote {
            git_ref: espidf::parse_esp_idf_git_ref(&self.version),
            repo_url: Some("https://github.com/espressif/esp-idf".to_string()),
        };

        let espidf_origin = espidf::EspIdfOrigin::Managed(repo.clone());
        install(espidf_origin)?;
        let espidf_dir = get_install_path(repo);
        if minify {
            info!("{} Minifying ESP-IDF", emoji::INFO);
            fs::remove_dir_all(espidf_dir.join("docs"))?;
            fs::remove_dir_all(espidf_dir.join("examples"))?;
            fs::remove_dir_all(espidf_dir.join("tools").join("esp_app_trace"))?;
            fs::remove_dir_all(espidf_dir.join("tools").join("test_idf_size"))?;
        }
        Ok(espidf_dir)
    }

    /// Create a new instance with the propper arguments.
    pub fn new(version: &str, minified: bool, targets: Vec<Chip>) -> EspIdf {
        let install_path = PathBuf::from(get_tools_path());
        debug!(
            "{} ESP-IDF install path: {}",
            emoji::DEBUG,
            install_path.display()
        );
        Self {
            repository_url: DEFAULT_GIT_REPOSITORY.to_string(),
            version: version.to_string(),
            minified,
            install_path,
            targets,
        }
    }
}

fn get_install_path(repo: EspIdfRemote) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    repo.repo_url.as_ref().unwrap().hash(&mut hasher);
    let repo_url_hash = format!("{:x}", hasher.finish());
    let repo_dir = match repo.git_ref {
        git::Ref::Branch(n) | git::Ref::Tag(n) | git::Ref::Commit(n) => n,
    };
    // Replace all directory separators with a dash `-`, so that we don't create
    // subfolders for tag or branch names that contain such characters.
    let repo_dir = repo_dir.replace(&['/', '\\'], "-");

    let mut install_path = PathBuf::from(get_tools_path());
    install_path = install_path.join(PathBuf::from(format!("esp-idf-{}", repo_url_hash)));
    install_path = install_path.join(PathBuf::from(repo_dir));
    install_path
}
