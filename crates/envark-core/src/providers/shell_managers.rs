use super::*;

pub fn initialization(ctx: &Context, manager: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        return None;
    }
    let (variable, fallback, file) = match manager {
        "nvm" => ("NVM_DIR", ".nvm", "nvm.sh"),
        "SDKMAN!" => ("SDKMAN_DIR", ".sdkman", "bin/sdkman-init.sh"),
        _ => return None,
    };
    let root = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(fallback));
    let path = root.join(file);
    path.is_file().then_some(path)
}

pub fn command(ctx: &Context, manager: &str, args: &[&str]) -> Result<CommandSpec> {
    let init = initialization(ctx, manager)
        .ok_or_else(|| Error::Unavailable(format!("{manager} initialization was not found.")))?;
    let bash = ctx
        .executable("bash")
        .ok_or_else(|| Error::Unavailable("Bash is required by this version manager.".into()))?;
    build_command(bash, init, &ctx.home, manager, args)
}

fn build_command(
    bash: PathBuf,
    init: PathBuf,
    home: &Path,
    manager: &str,
    args: &[&str],
) -> Result<CommandSpec> {
    // Only these constant adapter programs are interpreted. Every path and user value is an argument.
    let script = match manager {
        "nvm" => "source \"$1\" --no-use || exit; shift; nvm \"$@\"",
        "SDKMAN!" => {
            "source \"$1\" || exit; shift; sdkman_auto_answer=false; sdkman_colour_enable=false; sdk \"$@\" <<< n"
        }
        _ => return Err(Error::InvalidInput("Unknown shell manager.".into())),
    };
    let mut spec = CommandSpec::new(
        bash,
        ["--noprofile", "--norc", "-c", script, "envark-manager"],
    );
    spec.args.push(init.to_string_lossy().into_owned());
    spec.args.extend(args.iter().map(|s| s.to_string()));
    spec.env.insert("BASH_ENV".into(), "/dev/null".into());
    spec.cwd = Some(home.into());
    Ok(spec)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shell_managers_pass_paths_and_values_as_literal_arguments() {
        let root = tempfile::tempdir().unwrap();
        let bash = which::which("bash").unwrap();
        let token = CancellationToken::new();
        for (manager, function) in [("nvm", "nvm"), ("SDKMAN!", "sdk")] {
            let init = root.path().join("manager with 'quotes'.sh");
            std::fs::write(&init, format!("{function}() {{ printf '%s\\n' \"$@\"; }}")).unwrap();
            let value = "21; touch unexpected-file";
            let command = build_command(
                bash.clone(),
                init,
                root.path(),
                manager,
                &["install", value],
            )
            .unwrap();
            let output = Runner::default().run(&command, &token).await.unwrap();
            assert_eq!(output.stdout, format!("install\n{value}\n"));
            assert!(!root.path().join("unexpected-file").exists());
        }
    }
}
