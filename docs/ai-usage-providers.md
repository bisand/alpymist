# AI usage: what each provider can and cannot say

Research for the AI usage widget (#16, [ADR 0017](adr/0017-ai-usage.md)), as of
2026-10-01. For each provider: what an individual can read about their own
limits or spend, with which credential, and whether the vendor documents it.

**How it was checked.** The documented routes below were read from the
vendors' own pages or specifications on that date, and the four readers for
them are tested against the documented example responses. The three vendors'
APIs were also asked from an Alpymist machine with made-up keys, and each
refused them in the documented way. No reader has yet been run with a real
key or a real login.

The undocumented routes were checked as far as they can be without an
account. ChatGPT's and Gemini CLI's fields are from the source of the tools
that ask them (`openai/codex`, `google-gemini/gemini-cli`), read on
2026-10-01. GitHub Copilot's are from other tools' published examples, since
GitHub's editors are not open. Each reader is tested against those shapes.

Two were then run against live answers, on 2026-10-01, from a Mac:

- **GitHub Copilot**, an individual paid plan: read correctly.
- **ChatGPT / Codex**, a business plan: the answer had `rate_limit: null` and
  a credit allowance under `spend_control.individual_limit`, which the reader
  now shows. No answer from a Plus or Pro plan, the ones with 5-hour and
  weekly windows, has been seen.
- **Gemini CLI** has not been run against a live answer.

## Summary

| Provider | What can be read | Credential | Documented | Shipped |
|---|---|---|---|---|
| OpenRouter | month's spend, the key's limit and what is left of it; credit balance | ordinary key; management key for the balance | yes | yes |
| Claude Pro/Max | 5-hour, weekly and spend-limit windows: % used, reset time | none: Claude Code hands them to its status line command | yes | yes |
| Claude Pro/Max | the same, at any time | Claude Code's own login | no, and against Anthropic's terms | no, and not planned |
| Anthropic API | spend per day; no balance | Admin key, organisations only | yes | yes |
| OpenAI API | spend per day; no balance | admin key | yes | yes |
| Mistral | spend against the spend limit | admin key | yes | not yet |
| GitHub Copilot | allowance, remaining, reset date | `gh`'s login, through `gh api` | no | yes, marked |
| ChatGPT / Codex | 5-hour and weekly % used, plan | Codex CLI's login | no | yes, marked |
| Gemini CLI | remaining fraction per model | Gemini CLI's login, while it lasts | no, reported unreliable | yes, marked |
| Gemini API key | nothing | — | — | not possible |

## Documented routes

### OpenRouter

- `GET https://openrouter.ai/api/v1/key`, `Authorization: Bearer <key>`. Under
  `data`: `usage`, `usage_daily`, `usage_weekly`, `usage_monthly` (USD, UTC
  periods), `limit` and `limit_remaining` (USD, or null with no limit),
  `limit_reset` (a kind of reset such as `"monthly"`, not a time),
  `is_free_tier`.
- `GET https://openrouter.ai/api/v1/credits` gives `data.total_credits` and
  `data.total_usage`, lifetime totals; the balance is the difference. The
  specification says a management key is required.
- No limit is documented on asking either.
- Source: `https://openrouter.ai/openapi.json`.

### Claude subscription, through Claude Code's status line

- Claude Code runs a status line command and hands it a JSON document on its
  standard input. For Pro and Max subscribers it carries `rate_limits`, with
  `five_hour`, `seven_day` and `spend_limit`, each `{used_percentage,
  resets_at}`: a number from 0 to 100, and Unix seconds.
- It is there only after a session's first answer, each window may be absent,
  and Claude Code drops a window once its reset has passed.
- The command is set in `~/.claude/settings.json`:
  `"statusLine": {"type": "command", "command": "…"}`.
- **What it cannot do:** the figures are as fresh as the last use of Claude
  Code on this account. Use from claude.ai or another machine is not seen
  until then.
- Source: `https://code.claude.com/docs/en/statusline`.

### Anthropic API

- `GET https://api.anthropic.com/v1/organizations/cost_report`, headers
  `x-api-key: <Admin key>` and `anthropic-version: 2023-06-01`. Parameters:
  `starting_at` (RFC 3339, required), `ending_at`, `bucket_width=1d`, `limit`
  (up to 31), `page`.
- Answers daily buckets, each with `results[].amount`, **a decimal string in
  cents** (`"123.45"` is $1.23), and `currency`. `has_more` and `next_page`
  paginate.
- Needs an Admin key (`sk-ant-admin…`), which only an organisation has: "The
  Admin API is unavailable for individual accounts."
- No endpoint gives the prepaid credit left. Anthropic asks for at most one
  asking a minute; figures are about five minutes behind.
- Source: `https://platform.claude.com/docs/en/manage-claude/usage-cost-api`.

### OpenAI API

- `GET https://api.openai.com/v1/organization/costs`, `Authorization: Bearer
  <admin key>`. Parameters: `start_time` (Unix seconds, required), `end_time`,
  `bucket_width=1d`, `limit` (up to 180), `page`.
- Answers daily buckets with `results[].amount.value`, **a number in dollars**,
  and `amount.currency`. `has_more` and `next_page` paginate.
- No documented endpoint gives the credit balance; the old
  `/dashboard/billing/credit_grants` now wants a browser session.
- Source: OpenAI's API reference, organization costs.

### Mistral

- An Admin API under `https://api.mistral.ai/v1/admin`, with `x-api-key: <Admin
  key>`, covers billing usage and the spend limit. Exact paths and fields were
  not read. An ordinary key has no usage endpoint, and Le Chat's personal
  limits have none either.
- Source: `https://docs.mistral.ai/admin/admin-api/usage-metrics`.

## Undocumented routes

These read a subscription through an endpoint the vendor's own tool uses,
with the login that tool saved. They work today and can stop without notice.
A provider's file marks them `unofficial = true`, and turning one on says so.

None of the three is handed a key or keeps one. Each uses the login the
vendor's own tool saved, reads it and never writes it, and does not renew it:
a login that has run out is said to have, and the last answer stays with its
age.

- **GitHub Copilot:** `gh api copilot_internal/user`, so the login never
  leaves `gh`. A paid plan answers `copilot_plan`, `quota_reset_date`, and
  `quota_snapshots` with `premium_interactions`, `chat` and `completions`,
  each `{entitlement, remaining, percent_remaining, unlimited}`. The free
  plan answers `limited_user_quotas` beside `monthly_quotas`, and
  `limited_user_reset_date`. The documented alternative,
  `GET /users/{username}/settings/billing/premium_request/usage`, gives usage
  only, with no allowance or reset.
- **ChatGPT / Codex:** `GET https://chatgpt.com/backend-api/wham/usage`,
  `Authorization: Bearer` the `tokens.access_token` in `~/.codex/auth.json`
  (or under `CODEX_HOME`), and `ChatGPT-Account-Id:` its `tokens.account_id`.
  Answers `plan_type`, and `rate_limit.primary_window` and
  `secondary_window`, each `{used_percent, limit_window_seconds,
  reset_after_seconds, reset_at}`: 0 to 100, and Unix seconds. Reported to
  disagree with the CLI's own `/status` at times. A workspace plan answers
  `rate_limit: null` and `spend_control.individual_limit` with `limit`,
  `used`, `remaining` (decimal strings, in credits), `used_percent` and
  `reset_at`. A Codex logged in with an
  API key has no such login, and is told so.
- **Gemini CLI:** two POSTs to `https://cloudcode-pa.googleapis.com/v1internal`
  with the `access_token` in `~/.gemini/oauth_creds.json`. `:loadCodeAssist`
  answers `cloudaicompanionProject` and the tier's name; `:retrieveUserQuota`
  with `{"project": …}` answers `buckets`, each `{modelId, remainingFraction,
  resetTime, tokenType}`. Reported to answer a fraction stuck at 1 at times.
  **The login lasts an hour.** Renewing it takes Gemini CLI's own OAuth
  client secret, which Alpymist does not use, so the figures are as fresh as
  Gemini CLI's last use.

## Not done, and why

- **Claude Code's login** (`~/.claude/.credentials.json`, against
  `api.anthropic.com/api/oauth/usage`) would give the subscription's limits at
  any time. Anthropic's terms keep that login for Claude Code and its own
  apps, and say third parties may not collect or use it. Alpymist does not
  read it.
- **A Gemini API key** has no usage or quota endpoint. Google's Cloud
  Monitoring and Service Usage APIs need a Cloud project and OAuth, which is
  not what someone with an AI Studio key has.
- **Remaining prepaid credit** for the Anthropic and OpenAI APIs: neither has
  an endpoint for it. The bar shows what was spent this month.
