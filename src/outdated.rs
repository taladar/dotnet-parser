//! This parses the output of dotnet-outdated
use std::process::Command;
use std::str::from_utf8;
use tracing::{debug, trace, warn};

/// should upgrades be locked to a specific major/minor/patch level only
#[derive(Debug, Clone, clap::ArgEnum)]
pub enum VersionLock {
    /// do not lock the version when considering upgrades
    None,
    /// lock the version to the current major version (i.e. only consider minor versions and patch levels)
    Major,
    /// lock the version to the current minor version (i.e. only consider patch levels)
    Minor,
}

impl Default for VersionLock {
    fn default() -> Self {
        VersionLock::None
    }
}

impl std::fmt::Display for VersionLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionLock::None => {
                write!(f, "None")
            }
            VersionLock::Major => {
                write!(f, "Major")
            }
            VersionLock::Minor => {
                write!(f, "Minor")
            }
        }
    }
}

/// Should dotnet-outdated look for pre-release versions of packages?
#[derive(Debug, Clone, clap::ArgEnum)]
pub enum PreRelease {
    /// Never look for pre-releases
    Never,
    /// automatically let dotnet-outdated determine if pre-releases are appropriate to look for
    Auto,
    /// Always look for pre-releases
    Always,
}

impl Default for PreRelease {
    fn default() -> Self {
        PreRelease::Auto
    }
}

impl std::fmt::Display for PreRelease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreRelease::Never => {
                write!(f, "Never")
            }
            PreRelease::Auto => {
                write!(f, "Auto")
            }
            PreRelease::Always => {
                write!(f, "Always")
            }
        }
    }
}

/// These are options to modify the behaviour of the program.
#[derive(Debug, Default, clap::Parser)]
pub struct DotnetOutdatedOptions {
    /// Include auto referenced packages
    #[clap(
        short = 'i',
        long = "include-auto-references",
        help = "Include auto-referenced packages"
    )]
    include_auto_references: bool,
    /// Should dotnet-outdated look for pre-release version of packages
    #[clap(
        long = "pre-release",
        value_name = "VALUE",
        default_value = "auto",
        help = "Should dotnet-outdated look for pre-release versions of packages",
        arg_enum
    )]
    pre_release: PreRelease,
    /// Dependencies that should be included in the consideration
    #[clap(
        long = "include",
        value_name = "PACKAGE_NAME",
        multiple_occurrences = true,
        number_of_values = 1,
        help = "Dependencies that should be included in the consideration"
    )]
    include: Vec<String>,
    /// Dependencies that should be excluded from consideration
    #[clap(
        long = "exclude",
        value_name = "PACKAGE_NAME",
        multiple_occurrences = true,
        number_of_values = 1,
        help = "Dependencies that should be excluded from consideration"
    )]
    exclude: Vec<String>,
    /// should transitive dependencies be considered
    #[clap(
        short = 't',
        long = "transitive",
        help = "Should dotnet-outdated consider transitiv dependencies"
    )]
    transitive: bool,
    /// if transitive dependencies are considered, to which depth
    #[clap(
        long = "transitive-depth",
        value_name = "DEPTH",
        default_value = "1",
        requires = "transitive",
        help = "If transitive dependencies are considered, to which depth in the dependency tree"
    )]
    transitive_depth: u64,
    /// should we consider all upgrades or limit to minor and/or patch levels only
    #[clap(
        long = "version-lock",
        value_name = "LOCK",
        default_value = "none",
        help = "Should we consider all updates or just minor versions and/or patch levels",
        arg_enum
    )]
    version_lock: VersionLock,
    /// path to pass to dotnet-outdated, defaults to current directory
    #[clap(
        long = "input-dir",
        value_name = "DIRECTORY",
        help = "The input directory to pass to dotnet outdated"
    )]
    input_dir: Option<std::path::PathBuf>,
}

/// Outer structure for parsing donet-outdated output
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DotnetOutdatedData {
    /// one per .csproj file (e.g. binaries, tests,...)
    pub projects: Vec<Project>,
}

/// Per project data
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Project {
    /// Name of the project
    pub name: String,
    /// absolute path to the .csproj file for it
    pub file_path: String,
    /// frameworks this targets with dependencies
    pub target_frameworks: Vec<Framework>,
}

/// Per project per target framework data
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Framework {
    /// Name of the framework, e.g. net5.0
    pub name: String,
    /// dependencies of the project when targeted for this framework
    pub dependencies: Vec<Dependency>,
}

/// Data about each outdated dependency
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Dependency {
    /// Name of the dependency
    pub name: String,
    /// the version that is currently in use
    pub resolved_version: String,
    /// the latest version as limited by the version lock parameter
    pub latest_version: String,
    /// severity of this upgrade
    pub upgrade_severity: Severity,
}

/// Severity of a required upgrade
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    /// a major version upgrade
    Major,
    /// a minor version uprade
    Minor,
    /// a patch level upgrade
    Patch,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Major => {
                write!(f, "Major")
            }
            Severity::Minor => {
                write!(f, "Minor")
            }
            Severity::Patch => {
                write!(f, "Patch")
            }
        }
    }
}

/// What the exit code indicated about required updates
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IndicatedUpdateRequirement {
    /// No update is required
    UpToDate,
    /// An update is required
    UpdateRequired,
}

impl std::fmt::Display for IndicatedUpdateRequirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndicatedUpdateRequirement::UpToDate => {
                write!(f, "up-to-date")
            }
            IndicatedUpdateRequirement::UpdateRequired => {
                write!(f, "update-required")
            }
        }
    }
}

/// main entry point for the dotnet-oudated call
pub fn outdated(
    options: &DotnetOutdatedOptions,
) -> Result<(IndicatedUpdateRequirement, DotnetOutdatedData), crate::Error> {
    let output_dir = tempfile::tempdir()?;
    let output_file = output_dir.path().join("outdated.json");
    let output_file = output_file
        .to_str()
        .ok_or(crate::Error::PathConversionError)?;

    let mut cmd = Command::new("dotnet");

    cmd.args([
        "outdated",
        "--fail-on-updates",
        "--output",
        output_file,
        "--output-format",
        "json",
    ]);

    if options.include_auto_references {
        cmd.args(["--include-auto-references"]);
    }

    cmd.args(["--pre-release", &options.pre_release.to_string()]);

    if !options.include.is_empty() {
        for i in &options.include {
            cmd.args(["--include", i]);
        }
    }

    if !options.exclude.is_empty() {
        for e in &options.exclude {
            cmd.args(["--exclude", e]);
        }
    }

    if options.transitive {
        cmd.args([
            "--transitive",
            "--transitive-depth",
            &options.transitive_depth.to_string(),
        ]);
    }

    cmd.args(["--version-lock", &options.version_lock.to_string()]);

    if let Some(ref input_dir) = options.input_dir {
        cmd.args([&input_dir]);
    }

    let output = cmd.output()?;

    if !output.status.success() {
        warn!(
            "dotnet outdated did not return with a successful exit code: {}",
            output.status
        );
        debug!("stdout:\n{}", from_utf8(&output.stdout)?);
        if !output.stderr.is_empty() {
            warn!("stderr:\n{}", from_utf8(&output.stderr)?);
        }
    }

    let update_requirement = if output.status.success() {
        IndicatedUpdateRequirement::UpToDate
    } else {
        IndicatedUpdateRequirement::UpdateRequired
    };

    let output_file_content = std::fs::read_to_string(output_file)?;

    trace!("Read output file content:\n{}", output_file_content);

    let jd = &mut serde_json::Deserializer::from_str(&output_file_content);
    let data: DotnetOutdatedData = serde_path_to_error::deserialize(jd)?;
    Ok((update_requirement, data))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Error;

    /// this test requires a .sln and/or .csproj files in the current
    /// directory (working dir of the tests)
    #[test]
    fn test_run_dotnet_outdated() -> Result<(), Error> {
        outdated(&Default::default())?;
        Ok(())
    }
}
