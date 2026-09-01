// story.rs 测试分片 01：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn safe_return_target_skips_exit_and_non_navigation_entries() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Menu",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Exit",
                tags: vec!["exit"],
                body: Vec::new(),
            },
        ],
    };

    let mut story: Story<'_, '_> = Story::new(&compiled);
    // Start（无导航）→ Menu（有导航）→ Exit（[exit] 标签，无导航）
    story.goto("Start").expect("Start 应可导航");
    story.record_navigation(false);
    story.goto("Menu").expect("Menu 应可导航");
    story.record_navigation(true);
    story.goto("Exit").expect("Exit 应可导航");
    story.record_navigation(false);

    // 当前位置是 Exit；[exit] 与无导航项都跳过，应回到 Menu。
    let target: &HirPassage<'_> = story
        .safe_return_target()
        .expect("应找到最近的可安全返回目标");
    assert_eq!(target.name, "Menu");
}

#[test]
fn story_loads_case_sensitive_passages_without_implied_navigation() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "start",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };

    let story: Story<'_, '_> = Story::new(&compiled);

    assert!(story.has("Start"));
    assert!(story.has("start"));
    assert!(!story.has("START"));
    assert_eq!(
        story.get("Start").map(|passage| passage.name),
        Some("Start")
    );
    assert_eq!(
        story.get("start").map(|passage| passage.name),
        Some("start")
    );
    assert_eq!(story.get("START"), None);
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

#[test]
fn story_init_is_found_exactly_but_cannot_become_navigation_history() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "StoryInit",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "storyinit",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);

    assert_eq!(
        story
            .story_init()
            .map(|passage: &HirPassage<'_>| passage.name),
        Some("StoryInit")
    );
    assert!(story.has("StoryInit"));
    assert_eq!(
        story.goto("StoryInit"),
        Err(StoryNavigationError::SpecialPassage(String::from(
            "StoryInit"
        )))
    );
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());

    let ordinary: &StoryHistoryEntry<'_, '_> = story
        .goto("storyinit")
        .expect("大小写不同的 Passage 仍是普通导航目标");
    assert_eq!(ordinary.passage().name, "storyinit");
}

#[test]
fn goto_confirms_existing_passages_and_keeps_failed_navigation_atomic() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);

    let start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可确认导航");
    assert_eq!(start.passage().name, "Start");
    let end: &StoryHistoryEntry<'_, '_> = story.goto("End").expect("End 应可确认导航");
    assert_eq!(end.passage().name, "End");
    let repeated: &StoryHistoryEntry<'_, '_> = story.goto("End").expect("重复访问仍应记录");
    assert_eq!(repeated.passage().name, "End");

    assert_eq!(story.current().map(|passage| passage.name), Some("End"));
    let history: Vec<&str> = story
        .history()
        .iter()
        .map(|entry| entry.passage().name)
        .collect();
    assert_eq!(history, vec!["Start", "End", "End"]);

    let error: StoryNavigationError = story
        .goto("Missing")
        .expect_err("缺失 Passage 不应确认导航");
    assert_eq!(
        error,
        StoryNavigationError::MissingPassage(String::from("Missing"))
    );
    assert_eq!(story.current().map(|passage| passage.name), Some("End"));
    assert_eq!(story.history().len(), 3);
}

#[test]
fn visits_counts_confirmed_history_with_case_sensitive_names() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "start",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let _first: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("首次 Start 应成功");
    let _lower: &StoryHistoryEntry<'_, '_> = story.goto("start").expect("小写 start 应独立成功");
    let _second: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("再次 Start 应成功");

    let upper_visits: usize = story.visits("Start");
    let lower_visits: usize = story.visits("start");
    let unmatched_visits: usize = story.visits("START");

    assert_eq!(upper_visits, 2);
    assert_eq!(lower_visits, 1);
    assert_eq!(unmatched_visits, 0);
    assert_eq!(story.history().len(), 3);
}

#[test]
fn visits_excludes_entries_after_the_history_cursor() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Hall",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Room",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    for _visit in 0..4 {
        story.goto("Hall").expect("Hall 应可导航");
        story.goto("Room").expect("Room 应可导航");
    }
    story.goto("Hall").expect("第五次 Hall 应可导航");

    assert_eq!(story.visits("Hall"), 5);
    story.back().expect("第五次 Hall 应可回退到 Room");
    story.back().expect("Room 应可回退到第四次 Hall");

    assert_eq!(story.current().map(|passage| passage.name), Some("Hall"));
    assert_eq!(story.visits("Hall"), 4);
    assert_eq!(story.history().len(), 9, "前进分支仍应保留");
}

#[test]
fn back_moves_the_history_cursor_without_deleting_forward_entries() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Map",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let initial_error: StoryHistoryError = story.back().expect_err("尚未导航时没有可回退位置");
    assert_eq!(initial_error, StoryHistoryError::NoPrevious);

    let _start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");
    let _map: &StoryHistoryEntry<'_, '_> = story.goto("Map").expect("Map 应可导航");
    let _end: &StoryHistoryEntry<'_, '_> = story.goto("End").expect("End 应可导航");

    let map: &StoryHistoryEntry<'_, '_> = story.back().expect("End 应可回退到 Map");
    assert_eq!(map.passage().name, "Map");
    assert_eq!(story.current().map(|passage| passage.name), Some("Map"));
    assert_eq!(story.position(), Some(1));
    let history: Vec<&str> = story
        .history()
        .iter()
        .map(|entry| entry.passage().name)
        .collect();
    assert_eq!(history, vec!["Start", "Map", "End"]);

    let start: &StoryHistoryEntry<'_, '_> = story.back().expect("Map 应可回退到 Start");
    assert_eq!(start.passage().name, "Start");
    let boundary_error: StoryHistoryError = story.back().expect_err("第一条历史不能继续回退");
    assert_eq!(boundary_error, StoryHistoryError::NoPrevious);
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
    assert_eq!(story.position(), Some(0));
    assert_eq!(story.history().len(), 3);
}

#[test]
fn forward_reuses_existing_history_without_creating_a_new_visit() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Map",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let initial_error: StoryHistoryError = story.forward().expect_err("尚未导航时没有可前进位置");
    assert_eq!(initial_error, StoryHistoryError::NoNext);

    let _start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");
    let _map: &StoryHistoryEntry<'_, '_> = story.goto("Map").expect("Map 应可导航");
    let _end: &StoryHistoryEntry<'_, '_> = story.goto("End").expect("End 应可导航");
    let _back_to_map: &StoryHistoryEntry<'_, '_> = story.back().expect("End 应可回退到 Map");
    let history_len: usize = story.history().len();

    let end: &StoryHistoryEntry<'_, '_> = story.forward().expect("Map 应可前进到 End");

    assert_eq!(end.passage().name, "End");
    assert_eq!(story.current().map(|passage| passage.name), Some("End"));
    assert_eq!(story.position(), Some(2));
    assert_eq!(story.history().len(), history_len);
    assert_eq!(story.visits("End"), 1);

    let boundary_error: StoryHistoryError = story.forward().expect_err("最后一条历史不能继续前进");
    assert_eq!(boundary_error, StoryHistoryError::NoNext);
    assert_eq!(story.current().map(|passage| passage.name), Some("End"));
    assert_eq!(story.position(), Some(2));
    assert_eq!(story.history().len(), history_len);
}

#[test]
fn goto_after_back_replaces_the_forward_branch_only_after_validation() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Map",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Shop",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let _start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");
    let _map: &StoryHistoryEntry<'_, '_> = story.goto("Map").expect("Map 应可导航");
    let _end: &StoryHistoryEntry<'_, '_> = story.goto("End").expect("End 应可导航");
    let _back_to_map: &StoryHistoryEntry<'_, '_> = story.back().expect("End 应可回退到 Map");

    let missing: StoryNavigationError =
        story.goto("Missing").expect_err("无效导航不应截断前进分支");
    assert_eq!(
        missing,
        StoryNavigationError::MissingPassage(String::from("Missing"))
    );
    assert_eq!(story.position(), Some(1));
    assert_eq!(story.history().len(), 3);
    let old_forward: &StoryHistoryEntry<'_, '_> =
        story.forward().expect("失败导航后 End 仍应可前进");
    assert_eq!(old_forward.passage().name, "End");
    let _back_again: &StoryHistoryEntry<'_, '_> = story.back().expect("End 应可再次回退到 Map");

    let shop: &StoryHistoryEntry<'_, '_> = story.goto("Shop").expect("有效新导航应建立分支");

    assert_eq!(shop.passage().name, "Shop");
    assert_eq!(story.current().map(|passage| passage.name), Some("Shop"));
    assert_eq!(story.position(), Some(2));
    let history: Vec<&str> = story
        .history()
        .iter()
        .map(|entry| entry.passage().name)
        .collect();
    assert_eq!(history, vec!["Start", "Map", "Shop"]);
    assert_eq!(story.visits("End"), 0);
    assert_eq!(story.visits("Shop"), 1);
    assert_eq!(story.forward(), Err(StoryHistoryError::NoNext));
}

#[test]
fn history_entries_keep_stable_ids_and_new_branches_do_not_reuse_them() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Branch",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let _start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");
    let _end: &StoryHistoryEntry<'_, '_> = story.goto("End").expect("End 应可导航");
    let end_id = story.history()[1].id();
    let _back: &StoryHistoryEntry<'_, '_> = story.back().expect("End 应可回退");
    let _forward: &StoryHistoryEntry<'_, '_> = story.forward().expect("Start 应可前进到 End");
    assert_eq!(story.current_entry().map(|entry| entry.id()), Some(end_id));

    let _back_again: &StoryHistoryEntry<'_, '_> = story.back().expect("End 应可再次回退");
    let _branch: &StoryHistoryEntry<'_, '_> = story.goto("Branch").expect("Branch 应建立新分支");
    let branch_id = story.history()[1].id();

    assert_ne!(branch_id, end_id);
    assert_eq!(
        story.current_entry().map(|entry| entry.id()),
        Some(branch_id)
    );
    assert_eq!(story.current().map(|passage| passage.name), Some("Branch"));
}

#[test]
fn reset_clears_navigation_without_reusing_history_ids() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let _start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");
    let _end: &StoryHistoryEntry<'_, '_> = story.goto("End").expect("End 应可导航");
    let old_end_id = story.history()[1].id();

    let removed: usize = story.reset();

    assert_eq!(removed, 2);
    assert_eq!(story.current(), None);
    assert_eq!(story.position(), None);
    assert!(story.history().is_empty());
    assert_eq!(story.visits("Start"), 0);
    assert_eq!(story.back(), Err(StoryHistoryError::NoPrevious));

    let restarted: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("reset 后应可重新启动");
    assert_ne!(restarted.id(), old_end_id);
    assert_eq!(story.history().len(), 1);
}

#[test]
fn history_get_only_resolves_ids_on_the_active_timeline() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "OldEnd",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "NewEnd",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let _start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");
    let start_id = story.history()[0].id();
    let _old_end: &StoryHistoryEntry<'_, '_> = story.goto("OldEnd").expect("OldEnd 应可导航");
    let old_end_id = story.history()[1].id();
    let _back: &StoryHistoryEntry<'_, '_> = story.back().expect("OldEnd 应可回退");
    let _new_end: &StoryHistoryEntry<'_, '_> = story.goto("NewEnd").expect("NewEnd 应建立新分支");
    let new_end_id = story.history()[1].id();
    let position_before: Option<usize> = story.position();

    assert_eq!(
        story
            .history_get(start_id)
            .map(|entry| entry.passage().name),
        Some("Start")
    );
    assert_eq!(story.history_get(old_end_id), None);
    assert_eq!(
        story
            .history_get(new_end_id)
            .map(|entry| entry.passage().name),
        Some("NewEnd")
    );
    assert_eq!(story.position(), position_before);
    assert_eq!(story.current().map(|passage| passage.name), Some("NewEnd"));
}

#[test]
fn navigation_actions_return_the_exact_confirmed_history_entry() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);

    let start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");
    let start_id = start.id();
    assert_eq!(start.passage().name, "Start");
    let end: &StoryHistoryEntry<'_, '_> = story.goto("End").expect("End 应可导航");
    let end_id = end.id();
    assert_eq!(end.passage().name, "End");

    let back: &StoryHistoryEntry<'_, '_> = story.back().expect("End 应可回退到 Start");
    assert_eq!(back.id(), start_id);
    assert_eq!(back.passage().name, "Start");
    let forward: &StoryHistoryEntry<'_, '_> = story.forward().expect("Start 应可前进到 End");
    assert_eq!(forward.id(), end_id);
    assert_eq!(forward.passage().name, "End");
}

#[test]
fn story_snapshot_restores_timeline_without_reusing_rolled_back_ids() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Map",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Failed",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Branch",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let _start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");
    let _map: &StoryHistoryEntry<'_, '_> = story.goto("Map").expect("Map 应可导航");
    let snapshot: StorySnapshot<'_, '_> = story.snapshot();
    let failed: &StoryHistoryEntry<'_, '_> = story.goto("Failed").expect("Failed 应先确认导航");
    let failed_id = failed.id();

    story.restore(snapshot).expect("同一 Story 的快照应可恢复");

    assert_eq!(story.current().map(|passage| passage.name), Some("Map"));
    assert_eq!(story.position(), Some(1));
    assert_eq!(story.history().len(), 2);
    assert_eq!(story.history_get(failed_id), None);

    let branch: &StoryHistoryEntry<'_, '_> = story.goto("Branch").expect("恢复后应可建立分支");
    assert_ne!(branch.id(), failed_id);
    assert_eq!(branch.passage().name, "Branch");
}
