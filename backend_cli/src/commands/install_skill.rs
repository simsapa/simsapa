//! `simsapa install-skill`: write the `simsapa` agent skill (`SKILL.md`,
//! compiled into the binary so it matches the installed version) into the skill
//! folder of Claude Code, OpenCode or Pi, for the user or for one project.
//! Works without databases.
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

pub const SKILL_NAME: &str = "simsapa";
const SKILL_FILE: &str = "SKILL.md";
const SKILL_TEMPLATE: &str = include_str!("../../skills/simsapa/SKILL.md");
/// In the frontmatter of every skill this command writes. A `SKILL.md` without
/// it is the user's own and is only replaced with `--force`.
const GENERATOR_MARKER: &str = "generator: simsapa-cli";

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

    /// Replace a SKILL.md that simsapa did not write
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

    /// The folder holding `SKILL.md`: `<agent config>/skills/simsapa`, with
    /// Pi's global skills one level down, under `agent/`.
    fn skill_dir(&self, agent: Agent, scope: Scope) -> Result<PathBuf, CliError> {
        let config = self.config_dir(agent, scope)?;
        let skills = match (agent, scope) {
            (Agent::Pi, Scope::Global) => config.join("agent").join("skills"),
            _ => config.join("skills"),
        };
        Ok(skills.join(SKILL_NAME))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Existing {
    Absent,
    /// Written by this command, same text.
    UpToDate,
    /// Written by this command, an older or different text.
    Outdated,
    /// A `SKILL.md` this command did not write.
    Foreign,
}

#[derive(Debug, Clone)]
pub struct Target {
    pub agent: Agent,
    pub scope: Scope,
    pub skill_dir: PathBuf,
    existing: Existing,
}

impl Target {
    fn file(&self) -> PathBuf {
        self.skill_dir.join(SKILL_FILE)
    }

    fn describe(&self) -> String {
        let state = match self.existing {
            Existing::Absent => "",
            Existing::UpToDate => " (already installed)",
            Existing::Outdated => " (will be updated)",
            Existing::Foreign => " (a SKILL.md not made by simsapa is there; --force replaces it)",
        };
        format!("{} ({}): {}{}", self.agent.label(), self.scope.label(), self.skill_dir.display(), state)
    }
}

/// The skill text for this build.
pub fn render_skill() -> String {
    SKILL_TEMPLATE.replace("{{version}}", get_app_version().trim_start_matches('v'))
}

fn path_exists(path: &Path) -> bool {
    matches!(path.try_exists(), Ok(true))
}

fn existing_state(skill_dir: &Path, rendered: &str) -> Result<Existing, CliError> {
    let file = skill_dir.join(SKILL_FILE);
    if !path_exists(&file) {
        return Ok(Existing::Absent);
    }
    let text = std::fs::read_to_string(&file)
        .map_err(|e| CliError::error(format!("Cannot read {}: {}", file.display(), e)))?;
    Ok(if text == rendered {
        Existing::UpToDate
    } else if text.contains(GENERATOR_MARKER) {
        Existing::Outdated
    } else {
        Existing::Foreign
    })
}

fn make_target(env: &SkillEnv, agent: Agent, scope: Scope, rendered: &str) -> Result<Target, CliError> {
    let skill_dir = env.skill_dir(agent, scope)?;
    let existing = existing_state(&skill_dir, rendered)?;
    Ok(Target { agent, scope, skill_dir, existing })
}

/// OpenCode reads Claude's skill folders as well, so with both selected for one
/// scope the OpenCode copy would be a duplicate of the same name. Returns what
/// was dropped.
fn drop_opencode_duplicates(targets: &mut Vec<Target>) -> Vec<Target> {
    let has_claude = |scope: Scope, ts: &[Target]| ts.iter().any(|t| t.agent == Agent::Claude && t.scope == scope);
    let snapshot = targets.clone();
    let mut dropped = Vec::new();
    targets.retain(|t| {
        let duplicate = t.agent == Agent::Opencode && has_claude(t.scope, &snapshot);
        if duplicate {
            dropped.push(t.clone());
        }
        !duplicate
    });
    dropped
}

/// What the arguments name, before any asking. `explicit` means both an agent
/// and a scope were given, so nothing is looked for and nothing is confirmed.
#[derive(Debug)]
pub struct Selection {
    pub targets: Vec<Target>,
    pub explicit: bool,
    pub dropped_duplicates: Vec<Target>,
    /// Detected, but holding a SKILL.md that is not ours (not explicit only).
    pub left_out_foreign: Vec<Target>,
}

pub fn select_targets(
    agents: &[Agent],
    global: bool,
    local: bool,
    force: bool,
    env: &SkillEnv,
    rendered: &str,
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
            let target = make_target(env, agent, scope, rendered)?;
            if !explicit && !force && target.existing == Existing::Foreign {
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
    path: String,
    status: &'static str,
}

fn write_target(target: &Target, rendered: &str) -> Result<&'static str, CliError> {
    if target.existing == Existing::UpToDate {
        return Ok("unchanged");
    }
    std::fs::create_dir_all(&target.skill_dir)
        .map_err(|e| CliError::error(format!("Cannot create {}: {}", target.skill_dir.display(), e)))?;
    let file = target.file();
    std::fs::write(&file, rendered)
        .map_err(|e| CliError::error(format!("Cannot write {}: {}", file.display(), e)))?;
    Ok(match target.existing {
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
        .with_prompt("Install the simsapa skill into (space toggles, enter confirms)")
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
    let rendered = render_skill();
    let selection = select_targets(&args.agent, args.global, args.local, args.force, &env, &rendered)?;

    for t in &selection.left_out_foreign {
        errln!("Left out: {}", t.describe());
    }
    for t in &selection.dropped_duplicates {
        errln!(
            "Not installing for OpenCode ({}): it reads the Claude Code skill folder installed alongside.",
            t.scope.label()
        );
    }

    let mut targets = selection.targets;
    if selection.explicit {
        let foreign: Vec<&Target> = targets.iter().filter(|t| t.existing == Existing::Foreign).collect();
        if !foreign.is_empty() && !args.force {
            let list: Vec<String> = foreign.iter().map(|t| format!("  {}", t.file().display())).collect();
            return Err(CliError::error(format!(
                "A SKILL.md that simsapa did not write is already here; --force replaces it:\n{}",
                list.join("\n")
            )));
        }
    } else {
        if targets.is_empty() && selection.left_out_foreign.is_empty() {
            return Err(none_found_error(&env));
        }
        if targets.is_empty() {
            return Err(CliError::error("Nothing to install: the places found hold a SKILL.md that simsapa did not write. Name one with --agent and --global/--local, and --force to replace it."));
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
        let status = write_target(target, &rendered)?;
        done.push(Installed {
            agent: target.agent.api_name(),
            scope: target.scope.api_name(),
            path: target.file().to_string_lossy().into_owned(),
            status,
        });
    }

    if json {
        return print_json(&done);
    }
    for (target, d) in targets.iter().zip(&done) {
        let verb = match d.status {
            "installed" => "Installed",
            "updated" => "Updated",
            _ => "Already up to date",
        };
        outln!("{} for {} ({}): {}", verb, target.agent.label(), target.scope.label(), d.path);
    }
    outln!("Start a new agent session to load it.");
    Ok(())
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
        select_targets(agents, global, local, false, e, &render_skill()).unwrap()
    }

    #[test]
    fn test_skill_text_is_valid_and_self_contained() {
        let text = render_skill();
        assert!(!text.contains("{{"), "an unfilled placeholder");
        assert!(text.starts_with("---\nname: simsapa\ndescription: "));
        assert!(text.contains(GENERATOR_MARKER));
        // The name rule shared by the three agents.
        assert!(SKILL_NAME.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        let description = text.lines().find(|l| l.starts_with("description: ")).unwrap();
        assert!(description.len() - "description: ".len() <= 1024);
        // Read outside this repository, like the bundled reference.
        for needle in ["](./", "CLAUDE.md", "AGENTS.md", "PROJECT_MAP", "backend/src", "tasks/", "docs/"] {
            assert!(!text.contains(needle), "repo reference {:?}", needle);
        }
    }

    #[test]
    fn test_skill_commands_exist() {
        // Every command the skill names is a real subcommand.
        use clap::CommandFactory;
        let cmd = crate::Cli::command();
        for name in ["search", "sutta", "word", "lookup", "gloss", "docs", "health", "serve", "setup"] {
            assert!(cmd.find_subcommand(name).is_some(), "no `{}` command", name);
        }
        assert!(SKILL_TEMPLATE.contains("--uid-prefix"));
    }

    #[test]
    fn test_explicit_paths_per_agent_and_scope() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        let g = pick(&[Agent::Claude, Agent::Opencode, Agent::Pi], true, false, &e);
        let paths: Vec<PathBuf> = g.targets.iter().map(|t| t.skill_dir.clone()).collect();
        // OpenCode is dropped as a duplicate of Claude's folder.
        assert_eq!(paths, vec![h.path().join(".claude/skills/simsapa"), h.path().join(".pi/agent/skills/simsapa")]);
        assert_eq!(g.dropped_duplicates.len(), 1);

        let l = pick(&[Agent::Opencode, Agent::Pi], false, true, &e);
        let paths: Vec<PathBuf> = l.targets.iter().map(|t| t.skill_dir.clone()).collect();
        assert_eq!(paths, vec![b.path().join(".opencode/skills/simsapa"), b.path().join(".pi/skills/simsapa")]);
    }

    #[test]
    fn test_xdg_config_home_moves_the_opencode_folder() {
        let (h, b) = dirs();
        let mut e = env(h.path(), b.path());
        e.xdg_config_home = Some(h.path().join("xdg"));
        let s = pick(&[Agent::Opencode], true, false, &e);
        assert_eq!(s.targets[0].skill_dir, h.path().join("xdg/opencode/skills/simsapa"));
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
        // Global first; OpenCode's global copy is dropped beside Claude's.
        assert_eq!(found, vec![(Agent::Claude, Scope::Global), (Agent::Pi, Scope::Local)]);
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
        assert!(select_targets(&[Agent::Claude], true, false, false, &e, "x").is_err());
        // Detection across both scopes just skips the global one.
        mk(&b.path().join(".claude"));
        let s = select_targets(&[], false, false, false, &e, "x").unwrap();
        assert_eq!(s.targets.len(), 1);
        assert_eq!(s.targets[0].scope, Scope::Local);
    }

    #[test]
    fn test_write_states_and_idempotence() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        let rendered = render_skill();
        let t = pick(&[Agent::Claude], true, false, &e).targets.remove(0);
        assert_eq!(t.existing, Existing::Absent);
        assert_eq!(write_target(&t, &rendered).unwrap(), "installed");
        assert_eq!(std::fs::read_to_string(t.file()).unwrap(), rendered);

        let t = pick(&[Agent::Claude], true, false, &e).targets.remove(0);
        assert_eq!(t.existing, Existing::UpToDate);
        assert_eq!(write_target(&t, &rendered).unwrap(), "unchanged");

        // An older build's text is replaced without --force.
        std::fs::write(t.file(), format!("---\nname: simsapa\nmetadata:\n  {}\n---\nold\n", GENERATOR_MARKER)).unwrap();
        let t = pick(&[Agent::Claude], true, false, &e).targets.remove(0);
        assert_eq!(t.existing, Existing::Outdated);
        assert_eq!(write_target(&t, &rendered).unwrap(), "updated");
        assert_eq!(std::fs::read_to_string(t.file()).unwrap(), rendered);
    }

    #[test]
    fn test_a_foreign_skill_is_not_ours_to_replace() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        let dir = h.path().join(".claude/skills/simsapa");
        mk(&dir);
        std::fs::write(dir.join(SKILL_FILE), "my own notes").unwrap();

        let explicit = pick(&[Agent::Claude], true, false, &e);
        assert_eq!(explicit.targets[0].existing, Existing::Foreign);

        // Found by detection, it is left out of what is offered.
        let found = pick(&[], true, false, &e);
        assert!(found.targets.is_empty());
        assert_eq!(found.left_out_foreign.len(), 1);
        assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), "my own notes");
    }

    #[test]
    fn test_force_offers_a_foreign_skill_found_by_detection() {
        let (h, b) = dirs();
        let e = env(h.path(), b.path());
        let dir = h.path().join(".claude/skills/simsapa");
        mk(&dir);
        std::fs::write(dir.join(SKILL_FILE), "my own notes").unwrap();
        let s = select_targets(&[], true, false, true, &e, &render_skill()).unwrap();
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
        assert_eq!(std::fs::read_to_string(dir.join(SKILL_FILE)).unwrap(), render_skill());
    }
}
