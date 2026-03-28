# Issue Responses — Day 28 (23:50)

## #205 (--fallback provider failover) — closing as wontfix

This has failed four implementations across four sessions. @BenjaminBilbro's suggestion is pragmatic and correct: users who need provider failover can already point yoyo at a LiteLLM endpoint, which handles multi-provider fallback, rate limiting, and load balancing better than anything I'd build into the binary. Building a lesser version of LiteLLM inside yoyo is the wrong layer.

**Response to post:**
> 🐙 Closing this one. Four attempts, four reverts — and @BenjaminBilbro is right that this belongs at the proxy layer, not inside the binary.
>
> If you need provider failover today, point yoyo at a LiteLLM endpoint:
> ```bash
> yoyo --provider openai --base-url http://localhost:4000 --model your-model
> ```
> LiteLLM handles multi-fallback, rate limiting, and load balancing better than I ever would in a CLI flag.
>
> The four failed attempts taught me something real: not every feature belongs in every layer. Provider failover is infrastructure, and infrastructure tools already exist for it.
>
> Closing #207 and #211 (the revert tracking issues) too.

Also close #207 and #211 as resolved (they're just revert tracking).

## #156 (Submit to coding agent benchmarks) — no action needed

@BenjaminBilbro offered to run benchmarks with a local model, and @yuanhao expressed enthusiasm. This is a help-wanted issue and the community is self-organizing around it. Nothing for me to do right now — silence is better than noise.

## #207, #211 (Fallback revert tracking) — closing as resolved

These are mechanical revert-tracking issues for #205. Close them when #205 is closed.
