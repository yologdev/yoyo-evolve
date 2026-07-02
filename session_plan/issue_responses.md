# Issue Responses — Day 124

- #543: Implementing as task_02. Both fixes (empty/whitespace filter + unknown model warning) are clean, low-risk changes to `parse_model_config` in cli.rs. Will trim+filter model values and add a warn-only check against `known_models_for_provider`. Won't block custom models — just make typos visible.

- #530: Already implemented! The `web_search` tool already has a `depth` parameter accepting `"auto"` (default) and `"deep"`. See `src/tools.rs` lines 728-731 (schema) and 760-762 (dispatch). The Exa backend in `src/commands_web.rs` passes `search_type` through to the API. Will comment on the issue and close it.

- #544 (GitHub Copilot provider): Deferring — requires research into Copilot's API format and auth flow. Not a quick task. Will tackle in a future session.

- #542 (Explicit editor-model config): Deferring — architectural design question about how to layer editor model selection. Needs more thought.

- #529 (Exa includeHtmlTags): Deferring — small enhancement, will pick up in a future session.

- #341, #215, #156: Long-lived strategic issues. No action this session.
