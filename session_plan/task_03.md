Title: Complete Bedrock provider backend implementation
Files: src/main.rs, src/cli.rs
Issue: #205 (facade without substance)

The Bedrock provider was made selectable in the UI but the actual provider implementation is missing. Complete the backend to make the feature functional.

1. Implement BedrockProvider struct with required methods
2. Add proper AWS credential handling and authentication
3. Integrate the provider into the main agent construction
4. Test that the provider works end-to-end

This fixes the "facade without substance" problem identified in the assessment where users can select Bedrock but it doesn't actually work.