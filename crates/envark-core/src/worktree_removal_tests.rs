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
            false,
        )
        .await
        .unwrap()
    }
    async fn discard(&self, plan: operations::Plan) -> operations::OperationResult {
        operations::execute(
            plan,
            self.settings.clone(),
            self.ctx.clone(),
            silent_progress(),
            "discard".into(),
            true,
        )
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn explicit_discard_removes_reviewed_staged_unstaged_and_untracked_changes() {
    let fixture = Fixture::new();
    git_command(&fixture.linked, &["mv", "source.rs", "renamed source.rs"]);
    fs::write(fixture.linked.join("renamed source.rs"), "unstaged edit").unwrap();
    fs::write(fixture.linked.join("new file.txt"), "untracked content").unwrap();
    let plan = fixture.plan().await.unwrap();
    let changes = &plan.view.worktree_changes[0];
    assert_eq!(changes.path, fs::canonicalize(&fixture.linked).unwrap());
    assert!(changes.files.iter().any(|f| f.path == "renamed source.rs"
        && f.original_path.as_deref() == Some("source.rs")
        && f.status == "RM"));
    assert!(
        changes
            .files
            .iter()
            .any(|f| f.path == "new file.txt" && f.status == "??")
    );
    assert!(
        plan.view.items[0]
            .command
            .as_ref()
            .unwrap()
            .contains("--force")
    );
    let result = fixture.discard(plan).await;
    assert_eq!(result.items[0].status, "success", "{:?}", result.items);
    assert!(!fixture.linked.exists());
    assert_eq!(
        fs::read_to_string(fixture.main.join("source.rs")).unwrap(),
        "committed source"
    );
    git_command(
        &fixture.main,
        &["show-ref", "--verify", "refs/heads/feature"],
    );
}

#[tokio::test]
async fn discard_rejects_new_content_and_index_changes_after_review() {
    let fixture = Fixture::new();
    fs::write(fixture.linked.join("source.rs"), "reviewed changes").unwrap();
    let plan = fixture.plan().await.unwrap();
    fs::write(fixture.linked.join("new file.txt"), "not reviewed").unwrap();
    assert_eq!(fixture.discard(plan).await.items[0].status, "failed");
    assert!(fixture.linked.join("new file.txt").exists());

    // The worktree contents and status codes stay unchanged while the index changes.
    git_command(&fixture.linked, &["add", "source.rs"]);
    fs::write(fixture.linked.join("source.rs"), "working copy").unwrap();
    let plan = fixture.plan().await.unwrap();
    let before_status = git_command(&fixture.linked, &["status", "--porcelain"]);
    let blob = git_command(&fixture.main, &["rev-parse", "HEAD:.gitignore"]);
    git_command(
        &fixture.linked,
        &[
            "update-index",
            "--cacheinfo",
            "100644",
            blob.trim(),
            "source.rs",
        ],
    );
    assert_eq!(
        git_command(&fixture.linked, &["status", "--porcelain"]),
        before_status
    );
    let result = fixture.discard(plan).await;
    assert_eq!(result.items[0].status, "failed");
    assert!(result.items[0].message.contains("changed after review"));
    assert_eq!(
        fs::read_to_string(fixture.linked.join("source.rs")).unwrap(),
        "working copy"
    );
}

#[tokio::test]
async fn force_cannot_remove_newly_dirty_worktrees_or_bypass_locks_and_protection() {
    let fixture = Fixture::new();
    let plan = fixture.plan().await.unwrap();
    fs::write(fixture.linked.join("source.rs"), "not reviewed").unwrap();
    assert_eq!(fixture.discard(plan).await.items[0].status, "failed");
    let plan = fixture.plan().await.unwrap();
    git_command(
        &fixture.main,
        &["worktree", "lock", fixture.linked.to_str().unwrap()],
    );
    assert_eq!(fixture.discard(plan).await.items[0].status, "failed");
    git_command(
        &fixture.main,
        &["worktree", "unlock", fixture.linked.to_str().unwrap()],
    );
    let plan = fixture.plan().await.unwrap();
    let mut settings = fixture.settings.clone();
    settings
        .protected_projects
        .push(fs::canonicalize(&fixture.linked).unwrap());
    let result = operations::execute(
        plan,
        settings,
        fixture.ctx.clone(),
        silent_progress(),
        "discard".into(),
        true,
    )
    .await
    .unwrap();
    assert_eq!(result.items[0].status, "failed");
    assert!(fixture.linked.join("source.rs").exists());
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
async fn preserves_dirty_worktrees_by_default_and_rejects_locked_and_protected_worktrees() {
    let mut fixture = Fixture::new();
    fs::write(fixture.linked.join("source.rs"), "uncommitted edit").unwrap();
    let plan = fixture.plan().await.unwrap();
    assert_eq!(
        fixture.execute(plan, fixture.settings.clone()).await.items[0].status,
        "skipped"
    );
    assert_eq!(
        fs::read_to_string(fixture.linked.join("source.rs")).unwrap(),
        "uncommitted edit"
    );
    git_command(&fixture.linked, &["restore", "source.rs"]);
    fs::write(fixture.linked.join("untracked.txt"), "untracked source").unwrap();
    let plan = fixture.plan().await.unwrap();
    assert_eq!(
        fixture.execute(plan, fixture.settings.clone()).await.items[0].status,
        "skipped"
    );
    assert!(fixture.linked.join("untracked.txt").exists());
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

#[tokio::test]
async fn batch_removal_reviews_all_worktrees_before_mutation_and_revalidates_each_item() {
    let fixture = Fixture::new();
    let second = fixture.main.parent().unwrap().join("second");
    git_command(
        &fixture.main,
        &["worktree", "add", "-b", "second", second.to_str().unwrap()],
    );
    let inventory = fixture.inventory();
    let ids: Vec<_> = inventory.worktrees.iter().map(|w| w.id.clone()).collect();
    fs::write(second.join("untracked.txt"), "preserve").unwrap();
    let plan = operations::prepare(
        ActionRequest::RemoveWorktrees { ids: ids.clone() },
        &inventory,
        &fixture.settings,
        &fixture.ctx,
    )
    .await
    .unwrap();
    assert!(fixture.linked.exists() && second.exists());
    let result = fixture.execute(plan, fixture.settings.clone()).await;
    assert_eq!(
        result
            .items
            .iter()
            .filter(|i| i.status == "skipped")
            .count(),
        1
    );
    assert_eq!(
        result
            .items
            .iter()
            .filter(|i| i.status == "success")
            .count(),
        1
    );
    assert!(second.join("untracked.txt").exists());
    assert!(!fixture.linked.exists());
}

#[tokio::test]
async fn batch_removal_revalidates_each_item_after_review() {
    let fixture = Fixture::new();
    let second = fixture.main.parent().unwrap().join("second");
    git_command(
        &fixture.main,
        &["worktree", "add", "-b", "second", second.to_str().unwrap()],
    );
    let inventory = fixture.inventory();
    let ids: Vec<_> = inventory.worktrees.iter().map(|w| w.id.clone()).collect();
    let plan = operations::prepare(
        ActionRequest::RemoveWorktrees { ids },
        &inventory,
        &fixture.settings,
        &fixture.ctx,
    )
    .await
    .unwrap();
    assert_eq!(plan.view.items.len(), 2);
    fs::write(second.join("untracked.txt"), "preserve after review").unwrap();
    let result = fixture.execute(plan, fixture.settings.clone()).await;
    assert_eq!(
        result
            .items
            .iter()
            .filter(|i| i.status == "success")
            .count(),
        1
    );
    assert_eq!(
        result.items.iter().filter(|i| i.status == "failed").count(),
        1
    );
    assert!(second.join("untracked.txt").exists());
    assert!(!fixture.linked.exists());
    assert!(fixture.main.join("source.rs").exists());
}

#[tokio::test]
async fn batch_discard_removes_both_changed_and_clean_reviewed_worktrees() {
    let fixture = Fixture::new();
    let second = fixture.main.parent().unwrap().join("second");
    git_command(
        &fixture.main,
        &["worktree", "add", "-b", "second", second.to_str().unwrap()],
    );
    fs::write(fixture.linked.join("source.rs"), "reviewed edit").unwrap();
    let inventory = fixture.inventory();
    let plan = operations::prepare(
        ActionRequest::RemoveWorktrees {
            ids: inventory.worktrees.iter().map(|w| w.id.clone()).collect(),
        },
        &inventory,
        &fixture.settings,
        &fixture.ctx,
    )
    .await
    .unwrap();
    assert_eq!(plan.view.worktree_changes.len(), 1);
    let result = fixture.discard(plan).await;
    assert_eq!(result.items.len(), 2);
    assert!(
        result.items.iter().all(|item| item.status == "success"),
        "{:?}",
        result.items
    );
    assert!(!fixture.linked.exists() && !second.exists());
    assert!(fixture.main.join("source.rs").exists());
}
