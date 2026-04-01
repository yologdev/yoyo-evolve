Title: Implement SubAgentTool core functionality
Files: src/main.rs, src/hooks.rs
Issue: #180

Implement the basic SubAgentTool functionality that has been planned for 6+ sessions but never shipped. This is a core capability gap that prevents yoyo from doing multi-file editing work that Claude Code can do.

1. Define the SubAgentTool struct and basic interface
2. Implement the core sub-agent spawning logic
3. Add basic error handling and validation
4. Write tests to verify the tool works correctly

Focus on the minimal viable implementation - just enough to make multi-file editing possible without getting into advanced features or complex configuration. This addresses a key capability gap identified in the assessment.