# Optional model connections and source review

The companion works without a model. Authored hints, engine operations and review
are local. Open **Tutor · optional** to configure a text model for this browser tab.
The key, question, settings and response are held in memory. They are not part of
project, progress or workspace exports. Reloading forgets them.

Choose **Ollama native chat** with a complete `/api/chat` URL, or
**OpenAI-compatible chat** with the provider's complete chat-completions URL.
Supply a model available at that endpoint. HTTPS is required except for loopback
HTTP. The endpoint must allow this page's origin. The companion does not install a
proxy or alter your model service. A successful connection to a local preview does
not prove that the deployed site's origin will be allowed.

Write a question and select the source/draft or active assertions to include.
**Preview exact request** shows the destination and the entire JSON request body.
No request occurs until **Send this exact context**. Authorization, if supplied,
is sent separately to that endpoint. Browser cookies and referrers are omitted;
redirects fail. There is no automatic retry or hidden conversation history.

Native Ollama requests explicitly disable streaming and request at most 512 output
tokens. Compatible requests use the common text fields and `store: false`, without
assuming that token-limit parameters work identically across providers. Both paths
bound response bytes and elapsed browser time. Cancellation discards partial text;
it does not guarantee that a provider stops work or charges. Use the provider's
own account controls for spending limits. Compatibility and storage behavior are
provider responsibilities, not established by sending these fields.

Replies are displayed as untrusted plain text. Tool calls and refusals are rejected.
A reply cannot grade an activity or admit knowledge. **Copy suggestion to review
editor** only copies text into the local draft. Review it against the original
source, including roles, scope, quantifiers and negation. Syntax can be valid while
meaning is wrong.

**Prepare readings and admission check** runs the pinned parser, semantic checks,
canonical round-trip, deterministic backtranslation and admission into a detached
copy of the current session. Main knowledge stays unchanged. Inspect the reading,
then explicitly reject or approve the exact draft for admission. The worker binds
approval to the source, formal bytes, engine, profile and knowledge epoch/revision;
intervening edits or changes require a fresh review. Approval is consumed on an
admission attempt. Restarting the worker invalidates it.

Unfinished drafts are saved with their project, including empty fields while
editing. Review tickets and approval are never saved. After admission, the original
source, source identity, text hashes and review-base identity remain alongside the
engine assertion. This is learner-attested provenance. Hashes detect disagreement
with those bytes; they do not authenticate a source or prove that review occurred
when importing someone else's project. A knowledge result is still conditional on
the chosen encoding and accepted evidence.

Protocol references: [Ollama chat](https://docs.ollama.com/api/chat),
[Ollama compatibility](https://docs.ollama.com/api/openai-compatibility), and
[OpenAI Chat Completions](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create).

For deterministic transport failures and review controls, run `just test-browser`.
The separate `tests/live-model.mjs` smoke requires a locally installed `llama3.1:8b`
and a served companion at `http://127.0.0.1:4182`. It sends only its visible fictional
inspection prompt, checks both local protocols plus a missing model, and writes a
JSON observation report. It is deliberately excluded from CI and never uses keys.
