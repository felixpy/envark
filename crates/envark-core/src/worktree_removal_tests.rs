use crate::{
    git,
    model::{Inventory, Settings, silent_progress},
    operations::{self, ActionRequest},
    providers::Context,
    scanner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tokio_util::sync::CancellationToken;

fn git_command(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .args([
            "-c",
            "core.hooksPath=",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

struct Fixture {
    _root: tempfile::TempDir,
    main: PathBuf,
    linked: PathBuf,
    settings: Settings,
    ctx: Context,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let main = root.path().join("main");
        let linked = root.path().join("linked checkout");
        fs::create_dir(&main).unwrap();
        git::init(&main);
        fs::write(main.join(".gitignore"), "node_modules/\n.env\n").unwrap();
        fs::write(main.join("source.rs"), "committed source").unwrap();
        git_command(&main, &["add", "."]);
        git_command(&main, &["commit", "-qm", "fixture"]);
        git::add_worktree(&main, &linked);
        fs::create_dir_all(linked.join("node_modules/pkg")).unwrap();
        fs::write(
            linked.join("node_modules/pkg/generated"),
            "generated fixture",
        )
        .unwrap();
        let settings = Settings {
            roots: vec![fs::canonicalize(&main).unwrap()],
            ..Default::default()
        };
        Self {
            _root: root,
            main,
            linked,
            settings,
            ctx: Context::new(CancellationToken::new()).unwrap(),
        }
    }
    fn inventory(&self) -> Inventory {
        let scanned =
            scanner::scan(&self.settings, &self.ctx.cancel, silent_progress(), "scan").unwrap();
        Inventory {
            projects: scanned.projects,
            worktrees: scanned.worktrees,
            ..Default::default()
        }
    }
    async fn plan(&self) -> crate::Result<operations::Plan> {
        let inventory = self.inventory();
        operations::prepare(
            ActionRequest::RemoveWorktree {
                id: inventory.worktrees[0].id.clone(),
            },
            &inventory,
            &self.settings,
            &self.ctx,
        )
        .await
    }
    async fn execute(
        &self,
        plan: operations::Plan,
        settings: Settings,
    ) -> operations::OperationResult {
        operations::execute(
            plan,
            settings,
            self.ctx.clone(),
            silent_progress(),
            "remove".into(),
        )
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn removes_a_clean_registered_checkout_and_preserves_its_branch_and_main_repository() {
    let fixture = Fixture::new();
    let inventory = fixture.inventory();
    let size = inventory.worktrees[0].size.as_ref().unwrap();
    assert!(size.complete);
    assert!(size.bytes >= 17 + 16);
    let plan = fixture.plan().await.unwrap();
    assert_eq!(plan.view.items[0].bytes, size.bytes);
    assert!(
        !plan.view.items[0]
            .command
            .as_ref()
            .unwrap()
            .contains("--force")
    );
    assert!(
        plan.view
            .warnings
            .iter()
            .any(|w| w.contains("ignored files"))
    );
    let result = fixture.execute(plan, fixture.settings.clone()).await;
    assert_eq!(result.items[0].status, "success", "{:?}", result.items);
    assert_eq!(result.removed_bytes, size.bytes);
    assert!(!fixture.linked.exists());
    assert!(fixture.main.join("source.rs").is_file());
    assert!(
        !git_command(&fixture.main, &["worktree", "list", "--porcelain"])
            .contains("linked checkout")
    );
    git_command(
        &fixture.main,
        &["show-ref", "--verify", "refs/heads/feature"],
    );
}

#[tokio::test]
async fn rejects_dirty_untracked_locked_and_protected_worktrees() {
    let mut fixture = Fixture::new();
    fs::write(fixture.linked.join("source.rs"), "uncommitted edit").unwrap();
    assert!(
        fixture
            .plan()
            .await
            .unwrap_err()
            .to_string()
            .contains("uncommitted")
    );
    git_command(&fixture.linked, &["restore", "source.rs"]);
    fs::write(fixture.linked.join("untracked.txt"), "untracked source").unwrap();
    assert!(
        fixture
            .plan()
            .await
            .unwrap_err()
            .to_string()
            .contains("untracked")
    );
    fs::remove_file(fixture.linked.join("untracked.txt")).unwrap();
    git_command(
        &fixture.main,
        &["worktree", "lock", fixture.linked.to_str().unwrap()],
    );
    assert!(fixture.plan().await.is_err());
    git_command(
        &fixture.main,
        &["worktree", "unlock", fixture.linked.to_str().unwrap()],
    );
    fixture
        .settings
        .protected_projects
        .push(fs::canonicalize(&fixture.linked).unwrap());
    assert!(fixture.plan().await.is_err());
    assert!(fixture.linked.join("source.rs").is_file());
}

#[tokio::test]
async fn revalidates_contents_scope_protection_and_locks_after_review() {
    let fixture = Fixture::new();
    let plan = fixture.plan().await.unwrap();
    fs::write(
        fixture.linked.join("node_modules/new-file"),
        "new ignored file",
    )
    .unwrap();
    assert_eq!(
        fixture.execute(plan, fixture.settings.clone()).await.items[0].status,
        "failed"
    );
    let plan = fixture.plan().await.unwrap();
    let mut settings = fixture.settings.clone();
    settings.roots.clear();
    assert_eq!(
        fixture.execute(plan, settings).await.items[0].status,
        "failed"
    );
    let plan = fixture.plan().await.unwrap();
    let mut settings = fixture.settings.clone();
    settings
        .protected_projects
        .push(fs::canonicalize(&fixture.linked).unwrap());
    assert_eq!(
        fixture.execute(plan, settings).await.items[0].status,
        "failed"
    );
    let plan = fixture.plan().await.unwrap();
    git_command(
        &fixture.main,
        &["worktree", "lock", fixture.linked.to_str().unwrap()],
    );
    assert_eq!(
        fixture.execute(plan, fixture.settings.clone()).await.items[0].status,
        "failed"
    );
    assert!(fixture.linked.join("source.rs").is_file());
}

#[tokio::test]
async fn revalidates_git_head_and_registration_after_review() {
    let fixture = Fixture::new();
    let plan = fixture.plan().await.unwrap();
    git_command(
        &fixture.linked,
        &["commit", "--allow-empty", "-qm", "new commit"],
    );
    let result = fixture.execute(plan, fixture.settings.clone()).await;
    assert_eq!(result.items[0].status, "failed");
    assert!(result.items[0].message.contains("HEAD changed"));
    let plan = fixture.plan().await.unwrap();
    fs::write(fixture.linked.join(".git"), "gitdir: missing").unwrap();
    assert_eq!(
        fixture.execute(plan, fixture.settings.clone()).await.items[0].status,
        "failed"
    );
    assert!(fixture.linked.join("source.rs").is_file());
}

#[tokio::test]
async fn nested_repositories_and_cancellation_preserve_the_checkout() {
    let fixture = Fixture::new();
    git::init(&fixture.linked.join("vendor"));
    assert!(
        fixture
            .plan()
            .await
            .unwrap_err()
            .to_string()
            .contains("nested repository")
    );
    assert!(fixture.linked.join("vendor/.git").is_dir());
    let fixture = Fixture::new();
    let plan = fixture.plan().await.unwrap();
    fixture.ctx.cancel.cancel();
    let result = fixture.execute(plan, fixture.settings.clone()).await;
    assert!(result.cancelled);
    assert!(fixture.linked.join("source.rs").is_file());
}
