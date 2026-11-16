//! `simsapa install-skill`: write the agent skills (`SKILL.md` files, compiled
//! into the binary so they match the installed version) into the skill folder
//! of Claude Code, OpenCode or Pi, for the user or for one project, and make
//! each skill a slash command where the agent needs a file for that. Works
//! without databases.
//!
//! Slash commands, as each tool documents them:
//! - Claude Code: a skill folder creates `/<name>` by itself (and wins over a
//!   `commands/<name>.md` of the same name), so no command file is written.
//! - OpenCode: skills are not commands; a command is a markdown file in
//!   `commands/` of its config folder, whose body is the prompt. The file
//!   loads the skill and passes `$ARGUMENTS` on.
//! - Pi: skills only.
//!
//! Nothing is written to a place the user did not either name or confirm: with
//! both `--agent` and a scope the paths are explicit; otherwise the agents'
//! existing configuration folders are looked for and the user is asked.

use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};
use serde::Serialize;

use simsapa_backend::errln;
use simsapa_backend::update_checker::get_app_version;

use crate::exit::{CliError, CliResult};
use crate::output::{outln, print_json};

const SKILL_FILE: &str = "SKILL.md";
/// In the frontmatter of every skill, and in every command file, this command
/// writes. A file without it is the user's own and is only replaced with
/// `--force`.
const GENERATOR_MARKER: &str = "generator: simsapa-cli";

/// One embedded skill, and the text of the slash command that starts it in
/// OpenCode.
pub struct SkillDef {
    pub name: &'static str,
    template: &'static str,
    /// The `description` in the OpenCode command's frontmatter.
    command_description: &'static str,
}

pub const SKILLS: [SkillDef; 2] = [
    SkillDef {
        name: "simsapa",
        template: include_str!("../../skills/simsapa/SKILL.md"),
        command_description: "Search the Pāli canon, dictionaries and library books offline with the simsapa program",
    },
    SkillDef {
        name: "gloss-pali-passage",
        template: include_str!("../../skills/gloss-pali-passage/SKILL.md"),
        command_description: "Word-by-word analysis document of a Pāli passage or sutta reference (usage: [headwords|word-in-sentence] <text or reference>)",
    },
];

impl SkillDef {
    /// The skill text for this build.
    pub fn render(&self) -> String {
        self.template.replace("{{version}}", get_app_version().trim_start_matches('v'))
    }

    /// The OpenCode command file: the body is the prompt, `$ARGUMENTS` the
    /// text typed after the command.
    pub fn render_command(&self) -> String {
        format!(
            "---\ndescription: {}\n---\n<!-- {} version: {} -->\n\
             Load the `{}` skill and follow it for this request.\n\n\
             Arguments: $ARGUMENTS\n",
            self.command_description,
            GENERATOR_MARKER,
            get_app_version().trim_start_matches('v'),
            self.name
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Agent {
    Claude,
    Opencode,
    Pi,
}

impl Agent {
    const ALL: [Agent; 3] = [Agent::Claude, Agent::Opencode, Agent::Pi];

    fn label(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Opencode => "OpenCode",
            Agent::Pi => "Pi",
        }
    }

    fn api_name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Opencode => "opencode",
            Agent::Pi => "pi",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    Local,
}

impl Scope {
    fn label(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Local => "this folder",
        }
    }

    fn api_name(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Local => "local",
        }
    }
}

#[derive(Args, Debug)]
pub struct InstallSkillArgs {
    /// Install for this agent: claude, opencode or pi (repeatable). Without
    /// it, the agents whose configuration folder exists are offered.
    #[arg(long, value_enum, value_name = "AGENT")]
    pub agent: Vec<Agent>,

    /// Install in the user's own configuration (~/.claude, ~/.config/opencode,
    /// ~/.pi), for every project
    #[arg(long, conflicts_with = "local")]
    pub global: bool,

    /// Install in the current folder (.claude, .opencode, .pi), for this
    /// project only
    #[arg(long)]
    pub local: bool,

    /// The folder --local means, instead of the current one
    #[arg(long, value_name = "DIRECTORY_PATH")]
    pub dir: Option<PathBuf>,

    /// Install into everything found, without asking
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Replace a SKILL.md or command file that simsapa did not write
    #[arg(long)]
    pub force: bool,
}

/// Where the folders are looked for. A value, not a call to `std::env`, so the
/// tests never touch the real home folder.
#[derive(Debug, Clone)]
pub struct SkillEnv {
    pub home: Option<PathBuf>,
    /// `XDG_CONFIG_HOME`, when set and not empty.
    pub xdg_config_home: Option<PathBuf>,
    /// What `--local` means.
    pub base: PathBuf,
}

impl SkillEnv {
    fn from_process(dir: Option<&Path>) -> Result<SkillEnv, CliError> {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let base = match dir {
            Some(d) => d.to_path_buf(),
            None => std::env::current_dir()
                .map_err(|e| CliError::error(format!("Cannot read the current folder: {}", e)))?,
        };
        Ok(SkillEnv {
            home: var("HOME").or_else(|| var("USERPROFILE")),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            base,
        })
    }

    fn home(&self) -> Result<&Path, CliError> {
        self.home
            .as_deref()
            .ok_or_else(|| CliError::error("Cannot find the home folder (HOME is not set), so --global has nowhere to install."))
    }

    /// The agent's own configuration folder: the one whose presence says the
    /// agent is in use.
    fn config_dir(&self, agent: Agent, scope: Scope) -> Result<PathBuf, CliError> {
        Ok(match (agent, scope) {
            (Agent::Claude, Scope::Global) => self.home()?.join(".claude"),
            (Agent::Claude, Scope::Local) => self.base.join(".claude"),
            (Agent::Opencode, Scope::Global) => match &self.xdg_config_home {
                Some(x) => x.join("opencode"),
                None => self.home()?.join(".config").join("opencode"),
            },
            (Agent::Opencode, Scope::Local) => self.base.join(".opencode"),
            (Agent::Pi, Scope::Global) => self.home()?.join(".pi"),
            (Agent::Pi, Scope::Local) => self.base.join(".pi"),
        })
    }

    /// The folder holding a skill's `SKILL.md`: `<agent config>/skills/<name>`,
    /// with Pi's global skills one level down, under `agent/`.
    fn skill_dir(&self, agent: Agent, scope: Scope, name: &str) -> Result<PathBuf, CliError> {
        let config = self.config_dir(agent, scope)?;
        let skills = match (agent, scope) {
            (Agent::Pi, Scope::Global) => config.join("agent").join("skills"),
            _ => config.join("skills"),
        };
        Ok(skills.join(name))
    }

    /// The slash-command file of a skill, for the agents that need one
    /// (OpenCode: `<config>/commands/<name>.md`).
    fn command_file(&self, agent: Agent, scope: Scope, name: &str) -> Result<Option<PathBuf>, CliError> {
        Ok(match agent {
            Agent::Opencode => Some(self.config_dir(agent, scope)?.join("commands").join(format!("{}.md", name))),
            Agent::Claude | Agent::Pi => None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Existing {
    Absent,
    /// Written by this command, same text.
    UpToDate,
    /// Written by this command, an older or different text.
    Outdated,
    /// A file this command did not write.
    Foreign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Skill,
    Command,
}

/// One file to write.
#[derive(Debug, Clone)]
pub struct FileTarget {
    pub kind: Kind,
    pub name: &'static str,
    pub path: PathBuf,
    content: String,
    existing: Existing,
}

/// What goes into one agent's folder, for one scope.
#[derive(Debug, Clone)]
pub struct Target {
    pub agent: Agent,
    pub scope: Scope,
    pub files: Vec<FileTarget>,
}

impl Target {
    fn has_foreign(&self) -> bool {
        self.files.iter().any(|f| f.existing == Existing::Foreign)
    }

    fn all_up_to_date(&self) -> bool {
        self.files.iter().all(|f| f.existing == Existing::UpToDate)
    }

    fn describe(&self) -> String {
        let state = if self.has_foreign() {
            " (a file not made by simsapa is there; --force replaces it)"
        } else if self.files.iter().any(|f| f.existing == Existing::Outdated) {
            " (will be updated)"
        } else if self.all_up_to_date() {
            " (already installed)"
        } else {
            ""
        };
        let places: Vec<String> = self
            .files
            .iter()
            .filter(|f| f.kind == Kind::Skill)
            .map(|f| f.path.parent().unwrap_or(&f.path).display().to_string())
            .chain(
                self.files
                    .iter()
                    .filter(|f| f.kind == Kind::Command)
                    .map(|f| f.path.display().to_string()),
            )
            .collect();
        format!("{} ({}): {}{}", self.agent.label(), self.scope.label(), places.join(", "), state)
    }
}

fn path_exists(path: &Path) -> bool {
    matches!(path.try_exists(), Ok(true))
}

fn existing_state(file: &Path, rendered: &str) -> Result<Existing, CliError> {
    if !path_exists(file) {
        return Ok(Existing::Absent);
    }
    let text = std::fs::read_to_string(file)
        .map_err(|e| CliError::error(format!("Cannot read {}: {}", file.display(), e)))?;
    Ok(if text == rendered {
        Existing::UpToDate
    } else if text.contains(GENERATOR_MARKER) {
        Existing::Outdated
    } else {
        Existing::Foreign
    })
}

fn file_target(kind: Kind, def: &SkillDef, path: PathBuf, content: String) -> Result<FileTarget, CliError> {
    let existing = existing_state(&path, &content)?;
    Ok(FileTarget { kind, name: def.name, path, content, existing })
}

fn make_target(env: &SkillEnv, agent: Agent, scope: Scope) -> Result<Target, CliError> {
    let mut files = Vec::new();
    for def in SKILLS.iter() {
        let path = env.skill_dir(agent, scope, def.name)?.join(SKILL_FILE);
        files.push(file_target(Kind::Skill, def, path, def.render())?);
    }
    for def in SKILLS.iter() {
        if let Some(path) = env.command_file(agent, scope, def.name)? {
            files.push(file_target(Kind::Command, def, path, def.render_command())?);
        }
    }
    Ok(Target { agent, scope, files })
}

/// OpenCode reads Claude's skill folders as well, so with both selected for one
/// scope the OpenCode skill copies would duplicate the same names. They are
/// dropped; the OpenCode command files stay, because Claude's folder has no
/// commands for OpenCode to read. Returns the scopes where skills were dropped.
fn drop_opencode_duplicates(targets: &mut [Target]) -> Vec<Scope> {
    let claude_scopes: Vec<Scope> = targets.iter().filter(|t| t.agent == Agent::Claude).map(|t| t.scope).collect();
    let mut dropped = Vec::new();
    for t in targets.iter_mut() {
        if t.agent == Agent::Opencode && claude_scopes.contains(&t.scope) {
            t.files.retain(|f| f.kind != Kind::Skill);
            dropped.push(t.scope);
        }
    }
    dropped
}

/// What the arguments name, before any asking. `explicit` means both an agent
/// and a scope were given, so nothing is looked for and nothing is confirmed.
#[derive(Debug)]
pub struct Selection {
    pub targets: Vec<Target>,
    pub explicit: bool,
    /// The scopes whose OpenCode skill copies were left out beside Claude's.
    pub dropped_duplicates: Vec<Scope>,
    /// Detected, but holding a file that is not ours (not explicit only).
    pub left_out_foreign: Vec<Target>,
}

pub fn select_targets(
    agents: &[Agent],
    global: bool,
    local: bool,
    force: bool,
    env: &SkillEnv,
) -> Result<Selection, CliError> {
    let explicit = !agents.is_empty() && (global || local);
    let agent_list: Vec<Agent> = if agents.is_empty() {
        Agent::ALL.to_vec()
    } else {
        Agent::ALL.iter().copied().filter(|a| agents.contains(a)).collect()
    };
    let scopes: Vec<Scope> = match (global, local) {
        (true, _) => vec![Scope::Global],
        (_, true) => vec![Scope::Local],
        _ => vec![Scope::Global, Scope::Local],
    };

    let mut targets = Vec::new();
    let mut left_out_foreign = Vec::new();
    for &scope in &scopes {
        for &agent in &agent_list {
            // Without a home folder the global scope can only be skipped, and
            // is an error only when it was asked for by name.
            let config = match env.config_dir(agent, scope) {
                Ok(c) => c,
                Err(e) if global => return Err(e),
                Err(_) => continue,
            };
            if !explicit && !path_exists(&config) {
                continue;
            }
            let target = make_target(env, agent, scope)?;
            if !explicit && !force && target.has_foreign() {
                left_out_foreign.push(target);
            } else {
                targets.push(target);
            }
        }
    }

    let dropped_duplicates = drop_opencode_duplicates(&mut targets);
    Ok(Selection { targets, explicit, dropped_duplicates, left_out_foreign })
}

#[derive(Debug, Serialize)]
struct Installed {
    agent: &'static str,
    scope: &'static str,
    /// `skill` or `command`.
    kind: &'static str,
    name: &'static str,
    path: String,
    status: &'static str,
}

fn write_file(file: &FileTarget) -> Result<&'static str, CliError> {
    if file.existing == Existing::UpToDate {
        return Ok("unchanged");
    }
    if let Some(dir) = file.path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| CliError::error(format!("Cannot create {}: {}", dir.display(), e)))?;
    }
    std::fs::write(&file.path, &file.content)
        .map_err(|e| CliError::error(format!("Cannot write {}: {}", file.path.display(), e)))?;
    Ok(match file.existing {
        Existing::Absent => "installed",
        _ => "updated",
    })
}

fn none_found_error(env: &SkillEnv) -> CliError {
    let home = env.home.as_deref().map(|h| h.display().to_string()).unwrap_or_else(|| "~".into());
    CliError::error(format!(
        "No Claude Code, OpenCode or Pi configuration found in {} or {}. Name where to install it, \
         for example `simsapa install-skill --agent claude --global` or `--agent pi --local`.",
        home,
        env.base.display()
    ))
}

/// Ask which of the found places to install into. All start selected.
fn ask(targets: &[Target]) -> Result<Vec<Target>, CliError> {
    let labels: Vec<String> = targets.iter().map(|t| t.describe()).collect();
    let defaults = vec![true; targets.len()];
    let chosen = dialoguer::MultiSelect::new()
        .with_prompt("Install the simsapa skills into (space toggles, enter confirms)")
        .items(&labels)
        .defaults(&defaults)
        .interact()
        .map_err(|e| {
            CliError::error(format!(
                "Cannot ask which to install into ({}). Run `simsapa install-skill --yes`, or name --agent and --global/--local.",
                e
            ))
        })?;
    Ok(chosen.into_iter().map(|i| targets[i].clone()).collect())
}

pub fn run(args: &InstallSkillArgs, json: bool) -> CliResult {
    let env = SkillEnv::from_process(args.dir.as_deref())?;
    let selection = select_targets(&args.agent, args.global, args.local, args.force, &env)?;

    for t in &selection.left_out_foreign {
        errln!("Left out: {}", t.describe());
    }
    for scope in &selection.dropped_duplicates {
        errln!(
            "Not installing the OpenCode skill folders ({}): OpenCode reads the Claude Code skill folder installed alongside. \
             Its slash commands are installed.",
            scope.label()
        );
    }

    let mut targets = selection.targets;
    if selection.explicit {
        let foreign: Vec<String> = targets
            .iter()
            .flat_map(|t| t.files.iter())
            .filter(|f| f.existing == Existing::Foreign)
            .map(|f| format!("  {}", f.path.display()))
            .collect();
        if !foreign.is_empty() && !args.force {
            return Err(CliError::error(format!(
                "A file that simsapa did not write is already here; --force replaces it:\n{}",
                foreign.join("\n")
            )));
        }
    } else {
        if targets.is_empty() && selection.left_out_foreign.is_empty() {
            return Err(none_found_error(&env));
        }
        if targets.is_empty() {
            return Err(CliError::error("Nothing to install: the places found hold a file that simsapa did not write. Name one with --agent and --global/--local, and --force to replace it."));
        }
        if !args.yes {
            if !io::stdin().is_terminal() {
                let list: Vec<String> = targets.iter().map(|t| format!("  {}", t.describe())).collect();
                return Err(CliError::error(format!(
                    "Found:\n{}\nThere is no terminal to ask on. Run `simsapa install-skill --yes` to install into all of them, \
                     or name --agent and --global/--local.",
                    list.join("\n")
                )));
            }
            targets = ask(&targets)?;
            if targets.is_empty() {
                errln!("Nothing selected; nothing installed.");
                return Ok(());
            }
        }
    }

    let mut done = Vec::new();
    for target in &targets {
        for file in &target.files {
            let status = write_file(file)?;
            done.push(Installed {
                agent: target.agent.api_name(),
                scope: target.scope.api_name(),
                kind: if file.kind == Kind::Skill { "skill" } else { "command" },
                name: file.name,
                path: file.path.to_string_lossy().into_owned(),
                status,
            });
        }
    }

    if json {
        return print_json(&done);
    }
    for d in &done {
        let verb = match d.status {
            "installed" => "Installed",
            "updated" => "Updated",
            _ => "Already up to date",
        };
        outln!("{} {} {} for {} ({}): {}", verb, d.kind, d.name, d.agent, d.scope, d.path);
    }
    let names: Vec<String> = SKILLS.iter().map(|d| format!("/{}", d.name)).collect();
    outln!("Slash commands: {} (Claude Code makes them from the skills; Pi has skills only).", names.join(", "));
    outln!("Start a new agent session to load them.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Are the installed skills this version's?
// ---------------------------------------------------------------------------

/// `SIMSAPA_NO_SKILL_CHECK=1` (or `true`) turns off the stderr warning about
/// outdated skills. `simsapa health` reports them regardless.
pub const NO_SKILL_CHECK_ENV: &str = "SIMSAPA_NO_SKILL_CHECK";

/// One file this command wrote, found where an agent reads it.
#[derive(Debug, Clone, Serialize)]
pub struct FoundSkillFile {
    pub agent: &'static str,
    pub scope: &'static str,
    /// `skill` or `command`.
    pub kind: &'static str,
    pub name: &'static str,
    pub path: String,
    /// The simsapa version recorded in the file, `null` when it has none.
    pub version: Option<String>,
    pub up_to_date: bool,
    /// The folder `--local` was run in (local scope only).
    #[serde(skip)]
    base: Option<PathBuf>,
    #[serde(skip)]
    agent_enum: Option<Agent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillsState {
    UpToDate,
    Outdated,
    NotInstalled,
}

/// The `skills` object of `simsapa health --json`.
#[derive(Debug, Clone, Serialize)]
pub struct SkillsStatus {
    pub state: SkillsState,
    /// This program's version, the one the skills should have.
    pub version: String,
    pub installed: Vec<FoundSkillFile>,
    /// The commands that update the outdated files, one per place.
    pub commands: Vec<String>,
}

/// The `version` in a skill's frontmatter, or in a command file's comment.
fn recorded_version(text: &str) -> Option<String> {
    text.lines().take(20).find_map(|line| {
        let rest = &line[line.find("version:")? + "version:".len()..];
        let v = rest.trim().trim_end_matches("-->").trim().trim_matches('"').trim();
        (!v.is_empty()).then(|| v.to_string())
    })
}

/// A file at `path` that this command wrote, compared with `rendered`.
/// `None` when absent, unreadable or not ours.
fn found_file(
    agent: Agent,
    scope: Scope,
    kind: Kind,
    def: &SkillDef,
    path: PathBuf,
    rendered: &str,
    base: Option<&Path>,
) -> Option<FoundSkillFile> {
    if !path_exists(&path) {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    if !text.contains(GENERATOR_MARKER) {
        return None;
    }
    Some(FoundSkillFile {
        agent: agent.api_name(),
        scope: scope.api_name(),
        kind: if kind == Kind::Skill { "skill" } else { "command" },
        name: def.name,
        path: path.to_string_lossy().into_owned(),
        version: recorded_version(&text),
        up_to_date: text == rendered,
        base: base.map(Path::to_path_buf),
        agent_enum: Some(agent),
    })
}

fn found_in(env: &SkillEnv, scope: Scope, out: &mut Vec<FoundSkillFile>) {
    let base = (scope == Scope::Local).then_some(env.base.as_path());
    for agent in Agent::ALL {
        for def in SKILLS.iter() {
            if let Ok(dir) = env.skill_dir(agent, scope, def.name) {
                out.extend(found_file(agent, scope, Kind::Skill, def, dir.join(SKILL_FILE), &def.render(), base));
            }
            if let Ok(Some(path)) = env.command_file(agent, scope, def.name) {
                out.extend(found_file(agent, scope, Kind::Command, def, path, &def.render_command(), base));
            }
        }
    }
}

/// Every skill and command file this command wrote that an agent started in
/// `env.base` would read: the global folders, and the local ones of
/// `env.base` and each folder above it (an agent may run in a subfolder of
/// the project). The home folder is not a local base: its `.claude` is the
/// global one.
pub fn find_installed(env: &SkillEnv) -> Vec<FoundSkillFile> {
    let mut found = Vec::new();
    found_in(env, Scope::Global, &mut found);
    for dir in env.base.ancestors() {
        if env.home.as_deref() == Some(dir) {
            continue;
        }
        let local = SkillEnv { base: dir.to_path_buf(), ..env.clone() };
        found_in(&local, Scope::Local, &mut found);
    }
    found
}

/// The command that rewrites every file of one place (scope and folder),
/// naming each agent found there, so that the Claude Code / OpenCode duplicate
/// rule applies as it did when they were installed.
fn update_command(files: &[&FoundSkillFile], scope: Scope, base: Option<&Path>, cwd: &Path) -> String {
    let mut cmd = String::from("simsapa install-skill");
    for agent in Agent::ALL {
        if files.iter().any(|f| f.agent_enum == Some(agent)) {
            cmd.push_str(" --agent ");
            cmd.push_str(agent.api_name());
        }
    }
    match scope {
        Scope::Global => cmd.push_str(" --global"),
        Scope::Local => {
            cmd.push_str(" --local");
            if let Some(b) = base.filter(|b| *b != cwd) {
                cmd.push_str(&format!(" --dir \"{}\"", b.display()));
            }
        }
    }
    cmd
}

pub fn skills_status(env: &SkillEnv) -> SkillsStatus {
    let installed = find_installed(env);
    let version = get_app_version().trim_start_matches('v').to_string();

    // Places in the order found: (scope, base).
    let mut places: Vec<(Scope, Option<PathBuf>)> = Vec::new();
    for f in &installed {
        let scope = if f.scope == "global" { Scope::Global } else { Scope::Local };
        let key = (scope, f.base.clone());
        if !places.contains(&key) {
            places.push(key);
        }
    }
    let commands: Vec<String> = places
        .iter()
        .filter_map(|(scope, base)| {
            let files: Vec<&FoundSkillFile> = installed
                .iter()
                .filter(|f| f.scope == scope.api_name() && f.base == *base)
                .collect();
            files
                .iter()
                .any(|f| !f.up_to_date)
                .then(|| update_command(&files, *scope, base.as_deref(), &env.base))
        })
        .collect();

    let state = if installed.is_empty() {
        SkillsState::NotInstalled
    } else if commands.is_empty() {
        SkillsState::UpToDate
    } else {
        SkillsState::Outdated
    };
    SkillsStatus { state, version, installed, commands }
}

/// The stderr warning for outdated skills, `None` when there is nothing to say.
pub fn outdated_warning(status: &SkillsStatus) -> Option<String> {
    if status.state != SkillsState::Outdated {
        return None;
    }
    let mut versions: Vec<String> = status
        .installed
        .iter()
        .filter(|f| !f.up_to_date)
        .map(|f| f.version.clone().unwrap_or_else(|| "unknown".to_string()))
        .collect();
    versions.sort();
    versions.dedup();
    let from = if versions.iter().all(|v| *v == status.version) {
        format!("differ from this version's ({})", status.version)
    } else {
        format!("are from simsapa {}, not {}", versions.join(", "), status.version)
    };
    Some(format!(
        "Warning: the installed simsapa agent skills {}; they may name commands or options that have changed. \
         Update them with:\n{}",
        from,
        status.commands.iter().map(|c| format!("  {}", c)).collect::<Vec<_>>().join("\n")
    ))
}

/// Print the outdated-skills warning on stderr, unless `SIMSAPA_NO_SKILL_CHECK`
/// is set. Never fails: a folder that cannot be read is skipped.
pub fn warn_if_outdated() {
    if std::env::var(NO_SKILL_CHECK_ENV).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true")) {
        return;
    }
    let Ok(env) = SkillEnv::from_process(None) else { return };
    if let Some(warning) = outdated_warning(&skills_status(&env)) {
        errln!("{}", warning);
    }
}

/// For `simsapa health`, from the current folder.
pub fn current_skills_status() -> Option<SkillsStatus> {
    SkillEnv::from_process(None).ok().map(|env| skills_status(&env))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(home: &Path, base: &Path) -> SkillEnv {
        SkillEnv { home: Some(home.to_path_buf()), xdg_config_home: None, base: base.to_path_buf() }
    }

    fn dirs() -> (tempfile::TempDir, tempfile::TempDir) {
        (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap())
    }

    fn mk(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
    }

    fn pick(agents: &[Agent], global: bool, local: bool, e: &SkillEnv) -> Selection {
        select_targets(agents, global, local, false, e).unwrap()
    }

    fn paths(t: &Target, kind: Kind) -> Vec<PathBuf> {
        t.files.iter().filter(|f| f.kind == kind).map(|f| f.path.clone()).collect()
    }

    #[test]
    fn test_skill_texts_are_valid_and_self_contained() {
        for def in SKILLS.iter() {
            let text = def.render();
            assert!(!text.contains("{{"), "an unfilled placeholder in {}", def.name);
            assert!(
                text.starts_with(&format!("---\nname: {}\ndescription: ", def.name)),
                "frontmatter of {}",
                def.name
            );
            assert!(text.contains(GENERATOR_MARKER));
            // The name rule shared by the three agents.
            assert!(def.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
            let description = text.lines().find(|l| l.starts_with("description: ")).unwrap();
            assert!(description.len() - "description: ".len() <= 1024);
            // Read outside this repository, like the bundled reference.
            for needle in ["](./", "CLAUDE.md", "AGENTS.md", "PROJECT_MAP", "backend/src", "tasks/", "docs/"] {
                assert!(!text.contains(needle), "repo reference {:?} in {}", needle, def.name);
            }
        }
        assert_eq!(SKILLS[0].name, "simsapa");
        assert_eq!(SKILLS[1].name, "gloss-pali-passage");
    }

    #[test]
    fn test_skill_commands_exist() {
        // Every command the skills name is a real subcommand.
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        for name in ["search", "sutta", "word", "lookup", "gloss", "docs", "health", "serve", "setup"] {
            assert!(cmd.find_subcommand(name).is_some(), "no `{}` command", name);
        }
        assert!(SKILLS[0].template.contains("--uid-prefix"));
        // The gloss skill only uses flags `simsapa gloss` has.
        let gloss = cmd.find_subcommand("gloss").unwrap();
        for flag in ["no-ai", "format", "gloss-format", "paragraphs", "edition"] {
            assert!(gloss.get_arguments().any(|a| a.get_long() == Some(flag)), "no --{}", flag);
            assert!(SKILLS[1].template.contains(&format!("--{}", flag)), "skill does not name --{}", flag);
        }
    }

    #[test]
    fn test_gloss_skill_forbids_its_own_llm_requests() {
        let text = SKILLS[1].render();
        assert!(text.contains("argument-hint:"));
        assert!(text.contains("Make no LLM API request of your own"));
        assert!(text.contains("simsapa gloss --no-ai --format"));
    }

    #[test]
    fn test_command_text_for_opencode() {
        for def in SKILLS.iter() {
            let text = def.render_command();
            assert!(text.starts_with("---\ndescription: "));
            assert!(text.contains(GENERATOR_MARKER));
            assert!(text.contains(&format!("`{}` skill", def.name)));
            assert!(text.contains("$ARGUMENTS"));
            assert!(!text.contains("{{"));
        }
    }

    #[test]
    fn test_explicit_paths_per_agent_and_scope() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        let g = pick(&[Agent::Claude, Agent::Opencode, Agent::Pi], true, false, &e);
        assert_eq!(g.targets.len(), 3);
        let claude = &g.targets[0];
        assert_eq!(claude.agent, Agent::Claude);
        assert_eq!(
            paths(claude, Kind::Skill),
            vec![
                h.path().join(".claude/skills/simsapa/SKILL.md"),
                h.path().join(".claude/skills/gloss-pali-passage/SKILL.md")
            ]
        );
        // A Claude Code skill is its own slash command: no command file.
        assert!(paths(claude, Kind::Command).is_empty());

        // OpenCode's skill copies are dropped beside Claude's, its commands stay.
        let oc = &g.targets[1];
        assert_eq!(oc.agent, Agent::Opencode);
        assert!(paths(oc, Kind::Skill).is_empty());
        assert_eq!(
            paths(oc, Kind::Command),
            vec![
                h.path().join(".config/opencode/commands/simsapa.md"),
                h.path().join(".config/opencode/commands/gloss-pali-passage.md")
            ]
        );
        assert_eq!(g.dropped_duplicates, vec![Scope::Global]);

        let pi = &g.targets[2];
        assert_eq!(paths(pi, Kind::Skill)[1], h.path().join(".pi/agent/skills/gloss-pali-passage/SKILL.md"));
        assert!(paths(pi, Kind::Command).is_empty());

        let l = pick(&[Agent::Opencode, Agent::Pi], false, true, &e);
        let oc = &l.targets[0];
        assert_eq!(paths(oc, Kind::Skill)[0], b.path().join(".opencode/skills/simsapa/SKILL.md"));
        assert_eq!(paths(oc, Kind::Command)[1], b.path().join(".opencode/commands/gloss-pali-passage.md"));
        assert_eq!(paths(&l.targets[1], Kind::Skill)[0], b.path().join(".pi/skills/simsapa/SKILL.md"));
        assert!(l.dropped_duplicates.is_empty());
    }

    #[test]
    fn test_xdg_config_home_moves_the_opencode_folder() {
        let (h, b) = dirs();
        let mut e = env(h.path(), b.path());
        e.xdg_config_home = Some(h.path().join("xdg"));
        let s = pick(&[Agent::Opencode], true, false, &e);
        assert_eq!(paths(&s.targets[0], Kind::Skill)[0], h.path().join("xdg/opencode/skills/simsapa/SKILL.md"));
        assert_eq!(paths(&s.targets[0], Kind::Command)[0], h.path().join("xdg/opencode/commands/simsapa.md"));
    }

    #[test]
    fn test_detection_offers_only_existing_configs() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        mk(&h.path().join(".claude"));
        mk(&h.path().join(".config/opencode"));
        mk(&b.path().join(".pi"));
        let s = pick(&[], false, false, &e);
        assert!(!s.explicit);
        let found: Vec<(Agent, Scope)> = s.targets.iter().map(|t| (t.agent, t.scope)).collect();
        // Global first; OpenCode's global target stays, for its commands.
        assert_eq!(
            found,
            vec![(Agent::Claude, Scope::Global), (Agent::Opencode, Scope::Global), (Agent::Pi, Scope::Local)]
        );
    }

    #[test]
    fn test_detection_with_a_scope_only_looks_there() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        mk(&h.path().join(".claude"));
        mk(&b.path().join(".pi"));
        let s = pick(&[], false, true, &e);
        assert_eq!(s.targets.len(), 1);
        assert_eq!(s.targets[0].agent, Agent::Pi);
    }

    #[test]
    fn test_nothing_detected_is_empty_not_a_guess() {
        let (h, b) = dirs();
        let s = pick(&[], false, false, &env(h.path(), b.path()));
        assert!(s.targets.is_empty());
    }

    #[test]
    fn test_agent_without_scope_is_still_detection() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        mk(&h.path().join(".pi"));
        let s = pick(&[Agent::Pi], false, false, &e);
        assert!(!s.explicit);
        assert_eq!(s.targets.len(), 1);
        assert_eq!(s.targets[0].scope, Scope::Global);
    }

    #[test]
    fn test_global_without_a_home_folder_is_an_error_only_when_asked() {
        let (_h, b) = dirs();
        let e = SkillEnv { home: None, xdg_config_home: None, base: b.path().to_path_buf() };
        assert!(select_targets(&[Agent::Claude], true, false, false, &e).is_err());
        // Detection across both scopes just skips the global one.
        mk(&b.path().join(".claude"));
        let s = select_targets(&[], false, false, false, &e).unwrap();
        assert_eq!(s.targets.len(), 1);
        assert_eq!(s.targets[0].scope, Scope::Local);
    }

    #[test]
    fn test_write_states_and_idempotence() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        let t = pick(&[Agent::Opencode], true, false, &e).targets.remove(0);
        assert_eq!(t.files.len(), 4);
        assert!(t.files.iter().all(|f| f.existing == Existing::Absent));
        for f in &t.files {
            assert_eq!(write_file(f).unwrap(), "installed");
            assert_eq!(std::fs::read_to_string(&f.path).unwrap(), f.content);
        }

        let t = pick(&[Agent::Opencode], true, false, &e).targets.remove(0);
        assert!(t.files.iter().all(|f| f.existing == Existing::UpToDate));
        for f in &t.files {
            assert_eq!(write_file(f).unwrap(), "unchanged");
        }

        // An older build's command text is replaced without --force.
        let cmd = &t.files.iter().find(|f| f.kind == Kind::Command).unwrap().path;
        std::fs::write(cmd, format!("---\ndescription: old\n---\n<!-- {} -->\nold\n", GENERATOR_MARKER)).unwrap();
        let t = pick(&[Agent::Opencode], true, false, &e).targets.remove(0);
        let f = t.files.iter().find(|f| f.kind == Kind::Command).unwrap();
        assert_eq!(f.existing, Existing::Outdated);
        assert_eq!(write_file(f).unwrap(), "updated");
        assert_eq!(std::fs::read_to_string(&f.path).unwrap(), f.content);
    }

    #[test]
    fn test_a_foreign_skill_is_not_ours_to_replace() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        let dir = h.path().join(".claude/skills/simsapa");
        mk(&dir);
        std::fs::write(dir.join(SKILL_FILE), "my own notes").unwrap();

        let explicit = pick(&[Agent::Claude], true, false, &e);
        assert!(explicit.targets[0].has_foreign());

        // Found by detection, it is left out of what is offered.
        let found = pick(&[], true, false, &e);
        assert!(found.targets.is_empty());
        assert_eq!(found.left_out_foreign.len(), 1);
        assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), "my own notes");
    }

    #[test]
    fn test_a_foreign_command_file_is_not_ours_to_replace() {
        let (h, b) = dirs();
        let dir = b.path().join(".opencode/commands");
        mk(&dir);
        std::fs::write(dir.join("simsapa.md"), "my own command").unwrap();
        let args = |force| InstallSkillArgs {
            agent: vec![Agent::Opencode],
            global: false,
            local: true,
            dir: Some(b.path().to_path_buf()),
            yes: false,
            force,
        };
        let _ = &h;
        assert!(run(&args(false), true).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("simsapa.md")).unwrap(), "my own command");
        assert!(run(&args(true), true).is_ok());
        assert_eq!(std::fs::read_to_string(dir.join("simsapa.md")).unwrap(), SKILLS[0].render_command());
        assert!(dir.join("gloss-pali-passage.md").exists());
    }

    #[test]
    fn test_force_offers_a_foreign_skill_found_by_detection() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        let dir = h.path().join(".claude/skills/simsapa");
        mk(&dir);
        std::fs::write(dir.join(SKILL_FILE), "my own notes").unwrap();
        let s = select_targets(&[], true, false, true, &e).unwrap();
        assert_eq!(s.targets.len(), 1);
        assert!(s.left_out_foreign.is_empty());
    }

    #[test]
    fn test_run_refuses_a_foreign_skill_without_force_and_replaces_with_it() {
        let (h, b) = dirs();
        let dir = b.path().join(".claude/skills/simsapa");
        mk(&dir);
        std::fs::write(dir.join(SKILL_FILE), "my own notes").unwrap();
        let args = |force| InstallSkillArgs {
            agent: vec![Agent::Claude],
            global: false,
            local: true,
            dir: Some(b.path().to_path_buf()),
            yes: false,
            force,
        };
        let _ = &h;
        assert!(run(&args(false), true).is_err());
        assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), "my own notes");
        assert!(run(&args(true), true).is_ok());
        assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), SKILLS[0].render());
        assert!(b.path().join(".claude/skills/gloss-pali-passage/SKILL.md").exists());
        // No command file for Claude Code.
        assert!(!b.path().join(".claude/commands").exists());
    }

    #[test]
    fn test_run_installs_claude_and_opencode_together() {
        let (_h, b) = dirs();
        let args = InstallSkillArgs {
            agent: vec![Agent::Claude, Agent::Opencode],
            global: false,
            local: true,
            dir: Some(b.path().to_path_buf()),
            yes: false,
            force: false,
        };
        assert!(run(&args, true).is_ok());
        assert!(b.path().join(".claude/skills/gloss-pali-passage/SKILL.md").exists());
        assert!(!b.path().join(".opencode/skills").exists());
        assert!(b.path().join(".opencode/commands/gloss-pali-passage.md").exists());
        assert!(b.path().join(".opencode/commands/simsapa.md").exists());
    }

    fn install(e: &SkillEnv, agents: &[Agent], global: bool) {
        for t in select_targets(agents, global, !global, false, e).unwrap().targets {
            for f in &t.files {
                write_file(f).unwrap();
            }
        }
    }

    #[test]
    fn test_recorded_version() {
        assert_eq!(recorded_version(&SKILLS[0].render()).as_deref(), Some(get_app_version().trim_start_matches('v')));
        assert_eq!(recorded_version(&SKILLS[1].render_command()).as_deref(), Some(get_app_version().trim_start_matches('v')));
        assert_eq!(recorded_version("---\nname: x\n---\n"), None);
    }

    #[test]
    fn test_skills_status_not_installed_and_up_to_date() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        let s = skills_status(&e);
        assert_eq!(s.state, SkillsState::NotInstalled);
        assert!(outdated_warning(&s).is_none());

        install(&e, &[Agent::Claude], true);
        let s = skills_status(&e);
        assert_eq!(s.state, SkillsState::UpToDate);
        assert_eq!(s.installed.len(), 2);
        assert!(s.commands.is_empty());
        assert!(outdated_warning(&s).is_none());
    }

    #[test]
    fn test_skills_status_outdated_names_the_update_command() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        install(&e, &[Agent::Claude, Agent::Opencode], true);
        let skill = h.path().join(".claude/skills/simsapa/SKILL.md");
        let old = SKILLS[0].render().replace(
            &format!("version: \"{}\"", get_app_version().trim_start_matches('v')),
            "version: \"0.9.0\"",
        );
        std::fs::write(&skill, old).unwrap();

        let s = skills_status(&e);
        assert_eq!(s.state, SkillsState::Outdated);
        // Both agents of the place, so OpenCode's skill copies stay dropped.
        assert_eq!(s.commands, vec!["simsapa install-skill --agent claude --agent opencode --global".to_string()]);
        let w = outdated_warning(&s).unwrap();
        assert!(w.contains("are from simsapa 0.9.0, not "), "{w}");
        assert!(w.contains("\n  simsapa install-skill --agent claude --agent opencode --global"), "{w}");

        // A foreign file is not ours to report.
        std::fs::write(&skill, "my own notes").unwrap();
        assert_eq!(skills_status(&e).state, SkillsState::UpToDate);
    }

    #[test]
    fn test_skills_status_finds_a_local_install_in_a_parent_folder() {
        let (h, b) = dirs();
        install(&env(h.path(), b.path()), &[Agent::Pi], false);
        let sub = b.path().join("notes/sub");
        mk(&sub);
        let e = env(h.path(), &sub);
        let skill = b.path().join(".pi/skills/simsapa/SKILL.md");
        std::fs::write(&skill, format!("---\nmetadata:\n  {}\n  version: \"0.9.0\"\n---\n", GENERATOR_MARKER)).unwrap();

        let s = skills_status(&e);
        assert_eq!(s.state, SkillsState::Outdated);
        assert_eq!(
            s.commands,
            vec![format!("simsapa install-skill --agent pi --local --dir \"{}\"", b.path().display())]
        );
        // From the project folder itself, no --dir.
        let s = skills_status(&env(h.path(), b.path()));
        assert_eq!(s.commands, vec!["simsapa install-skill --agent pi --local".to_string()]);
    }
}
