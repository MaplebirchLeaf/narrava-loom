// story.rs 测试分片 02：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn story_snapshot_rejects_a_different_compiled_story_without_mutation() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let first_compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "First",
            tags: Vec::new(),
            body: Vec::new(),
        }],
    };
    let second_compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Second",
            tags: Vec::new(),
            body: Vec::new(),
        }],
    };
    let first: Story<'_, '_> = Story::new(&first_compiled);
    let snapshot: StorySnapshot<'_, '_> = first.snapshot();
    let mut second: Story<'_, '_> = Story::new(&second_compiled);
    let _second_entry: &StoryHistoryEntry<'_, '_> = second.goto("Second").expect("Second 应可导航");

    let error: StorySnapshotError = second
        .restore(snapshot)
        .expect_err("不同编译 Story 的快照必须拒绝");

    assert_eq!(error, StorySnapshotError::DifferentStory);
    assert_eq!(second.current().map(|passage| passage.name), Some("Second"));
    assert_eq!(second.history().len(), 1);
}

#[test]
fn story_snapshot_advances_another_instances_history_id_high_water_mark() {
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
    let mut first: Story<'_, '_> = Story::new(&compiled);
    let start: &StoryHistoryEntry<'_, '_> = first.goto("Start").expect("Start 应可导航");
    let start_id = start.id();
    let snapshot: StorySnapshot<'_, '_> = first.snapshot();
    let mut second: Story<'_, '_> = Story::new(&compiled);

    second.restore(snapshot).expect("同一编译结果应可恢复");
    let end: &StoryHistoryEntry<'_, '_> = second.goto("End").expect("恢复后 End 应可导航");

    assert_ne!(end.id(), start_id);
    assert_eq!(second.history().len(), 2);
}

#[test]
fn navigation_request_validates_without_mutation_and_confirmation_commits() {
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
    let start_id = story.current_entry().expect("Start 应为 current").id();

    let request: StoryNavigationRequest<'_, '_> =
        story.request_goto("End").expect("End 请求应通过验证");

    assert_eq!(request.passage().name, "End");
    assert_eq!(
        story.current_entry().map(StoryHistoryEntry::id),
        Some(start_id)
    );
    assert_eq!(story.history().len(), 1);

    let end: &StoryHistoryEntry<'_, '_> =
        story.confirm_navigation(request).expect("End 请求应可确认");
    assert_eq!(end.passage().name, "End");
    assert_ne!(end.id(), start_id);
    assert_eq!(story.history().len(), 2);
}

#[test]
fn navigation_request_from_another_compiled_story_is_rejected_atomically() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let first_compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "First",
            tags: Vec::new(),
            body: Vec::new(),
        }],
    };
    let second_compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Second",
            tags: Vec::new(),
            body: Vec::new(),
        }],
    };
    let first: Story<'_, '_> = Story::new(&first_compiled);
    let request: StoryNavigationRequest<'_, '_> = first
        .request_goto("First")
        .expect("First 请求应通过自己的 Story 验证");
    let mut second: Story<'_, '_> = Story::new(&second_compiled);

    let error: StoryNavigationError = second
        .confirm_navigation(request)
        .expect_err("跨编译 Story 请求必须拒绝");

    assert_eq!(error, StoryNavigationError::DifferentStoryRequest);
    assert_eq!(second.current(), None);
    assert!(second.history().is_empty());
}

#[test]
fn runtime_story_adapter_keeps_goto_pending_until_engine_confirmation() {
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

    let request: StoryNavigationRequest<'_, '_> = {
        let mut runtime_story: StoryRuntimeRequests<'_, '_, '_> = StoryRuntimeRequests::new(&story);
        runtime_story.goto("End").expect("End 应成为 pending goto");
        assert!(runtime_story.pending_goto().is_some());
        assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
        assert_eq!(story.history().len(), 1);
        runtime_story.take_goto().expect("pending goto 应可取出")
    };

    let end: &StoryHistoryEntry<'_, '_> = story
        .confirm_navigation(request)
        .expect("Engine 应可确认 pending goto");
    assert_eq!(end.passage().name, "End");
    assert_eq!(story.history().len(), 2);
}

#[test]
fn runtime_story_adapter_rejects_invalid_and_duplicate_goto_requests() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "First",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Second",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let story: Story<'_, '_> = Story::new(&compiled);
    let mut runtime_story: StoryRuntimeRequests<'_, '_, '_> = StoryRuntimeRequests::new(&story);

    let missing: StoryRuntimeRequestError =
        runtime_story.goto("Missing").expect_err("缺失目标必须拒绝");
    assert_eq!(
        missing,
        StoryRuntimeRequestError::Navigation(StoryNavigationError::MissingPassage(String::from(
            "Missing"
        )))
    );
    assert!(runtime_story.pending_goto().is_none());

    runtime_story
        .goto("First")
        .expect("First 应成为 pending goto");
    let duplicate: StoryRuntimeRequestError = runtime_story
        .goto("Second")
        .expect_err("未消费请求不能被覆盖");
    assert_eq!(duplicate, StoryRuntimeRequestError::GotoAlreadyPending);
    assert_eq!(
        runtime_story
            .pending_goto()
            .map(|request| request.passage().name),
        Some("First")
    );
}

#[test]
fn story_exposes_passage_tags_without_assigning_game_semantics() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Market",
                tags: vec!["town", "outdoor"],
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Inn",
                tags: vec!["town", "indoor"],
                body: Vec::new(),
            },
        ],
    };
    let story: Story<'_, '_> = Story::new(&compiled);

    let towns: Vec<&str> = story
        .tagged("town")
        .map(|passage: &HirPassage<'_>| passage.name)
        .collect();

    assert_eq!(towns, vec!["Market", "Inn"]);
    assert!(story.get("Inn").expect("Inn 应存在").has_tag("indoor"));
    assert!(
        !story
            .get("Market")
            .expect("Market 应存在")
            .has_tag("indoor")
    );
}

#[test]
fn runtime_story_adapter_queues_includes_without_changing_navigation() {
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
                name: "First",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Second",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let _start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");

    let (first, second): (StoryIncludeRequest<'_, '_>, StoryIncludeRequest<'_, '_>) = {
        let mut runtime_story: StoryRuntimeRequests<'_, '_, '_> = StoryRuntimeRequests::new(&story);
        runtime_story
            .include("First")
            .expect("First 应进入 include 队列");
        runtime_story
            .include("Second")
            .expect("Second 应保持请求顺序");
        assert_eq!(runtime_story.pending_include_count(), 2);
        assert_eq!(
            runtime_story
                .pending_include()
                .map(|request: &StoryIncludeRequest<'_, '_>| request.passage().name),
            Some("First")
        );
        (
            runtime_story.take_include().expect("First 请求应可取出"),
            runtime_story.take_include().expect("Second 请求应可取出"),
        )
    };

    assert_eq!(first.passage().name, "First");
    assert_eq!(second.passage().name, "Second");
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
    assert_eq!(story.history().len(), 1);
}

#[test]
fn runtime_story_requests_detach_and_only_reattach_to_the_same_story() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Target",
            tags: Vec::new(),
            body: Vec::new(),
        }],
    };
    let other_compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Target",
            tags: Vec::new(),
            body: Vec::new(),
        }],
    };
    let story: Story<'_, '_> = Story::new(&compiled);
    let other_story: Story<'_, '_> = Story::new(&other_compiled);
    let pending = {
        let mut requests: StoryRuntimeRequests<'_, '_, '_> = StoryRuntimeRequests::new(&story);
        requests.include("Target").expect("include 应进入队列");
        requests.goto("Target").expect("goto 应进入队列");
        requests.into_pending()
    };

    assert_eq!(pending.pending_include_count(), 1);
    assert!(pending.has_goto());
    let pending = match StoryRuntimeRequests::from_pending(&other_story, pending) {
        Ok(_) => panic!("另一份编译结果不能接管暂停请求"),
        Err(error) => error.pending,
    };
    let mut restored: StoryRuntimeRequests<'_, '_, '_> =
        StoryRuntimeRequests::from_pending(&story, pending)
            .unwrap_or_else(|_| panic!("原 Story 应能重新附着请求"));

    assert_eq!(
        restored
            .take_include()
            .map(|request| request.passage().name),
        Some("Target")
    );
    assert_eq!(
        restored.take_goto().map(|request| request.passage().name),
        Some("Target")
    );
}
