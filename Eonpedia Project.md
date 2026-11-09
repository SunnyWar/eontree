# Eonpedia / EonTree — A Deep-Time Think Tank

## What EonTree Is

EonTree is an open knowledge platform for mapping humanity's hardest, longest-running problems — and the uncomfortable solutions usually needed to actually solve them.

Most think tanks look at short-term policy through a human-centric lens. EonTree takes the opposite approach: it treats today's crises as instances of a handful of root problems that have recurred for millennia, and it refuses to filter out solutions just because they're unconventional, politically awkward, or inconvenient for the non-human world. The goal is a durable, evidence-grounded map of *problem → root cause → existing solution → barrier → innovation* that anyone can read, challenge, and extend.

## The Problem EonTree Addresses

Knowledge about hard problems is scattered across academic silos, locked behind paywalls, diluted by motivated reasoning, or quietly suppressed because it's socially uncomfortable. EonTree exists to counter that by building a single, structured, auditable graph of reasoning — one where every claim has to earn its place.

EonTree organizes its five root problems as:

1. **Systemic Resource Scarcity** — physical and ecological limits on energy, materials, and habitat.
2. **Collective Action Failure** — the coordination problems (tragedy of the commons, security dilemmas) that block cooperative solutions even when they're known.
3. **Epistemic Decay** — the erosion of any shared, reliable way to agree on what's true.
4. **Biological Obsolescence** — the mismatch between human cognition, evolved for small tribes, and the scale of modern problems.
5. **Institutional Drift and Power Asymmetry** — the tendency of large institutions to entrench power and resist reform.

## How the Knowledge Graph Works

Every piece of content in EonTree is a **node** in a directed graph, typed as one of:

- **Root** — the foundational axioms the rest of the graph is built on.
- **Problem** — a concrete instance of one of the root problems above.
- **Unknown** — an explicit, named gap in human knowledge blocking progress.
- **Barrier** — the specific economic, political, or psychological obstacle standing in the way.
- **Innovation** — a proposed solution pathway, evaluated on its logic and evidence rather than how unsettling it sounds.

Each node must connect to a parent, so solutions are always traceable back to the problem they claim to solve — you can't publish an answer floating free of a question.

## Trust Without Gatekeepers

EonTree is openly editable, which creates an obvious problem: how do you keep an open graph credible? Two mechanisms work together to answer that.

**Cryptographic integrity.** Every node's identity is a cryptographic hash of its own content, so nodes can't be silently altered after the fact — an edit always produces a new, distinct node rather than overwriting history. Every submission is signed by its author, so authorship and provenance are verifiable rather than asserted.

**The Global Sentinel Protocol.** Before anything else happens, every submission passes through an ethical pre-filter. Its mandate is simple: a solution does not get to count as "progress" if it achieves that progress by quietly offloading suffering onto the non-human world or permanently degrading the biosphere. This check is a hard pass/fail gate — it runs first, and it runs regardless of how compelling the rest of the submission is.

**The Weighted Epistemic Jury.** Nodes that clear the Sentinel are then evaluated in parallel by multiple independent AI judges. Rather than a blunt up/down vote, each judge scores the submission across six dimensions — plausibility, strength of evidence, predictive power, novelty, explanatory power, and relevance to the problem it's answering. Those scores are combined into a single weighted score, and the bar a node has to clear scales with how foundational it is: root-level and barrier claims are held to a noticeably higher standard than exploratory "unknown" nodes, since a shaky foundation undermines everything built on it. Submissions that fall short aren't just rejected — they come back with the per-dimension reasoning so a contributor can see exactly where the argument needs work.

**Built-in devil's advocate.** Every problem or innovation that gets approved automatically spawns three counter-node stubs — an objection, a failure mode, and a competing alternative — and adds them to a public queue. Anyone can claim one of these stubs and flesh it out with real argumentation, which means the graph is structurally biased toward self-criticism rather than one-sided advocacy.

## What's Actually Built

EonTree is implemented as an open-source Rust workspace, organized into four crates:

- **`eontree-core`** — the node data model, cryptographic signing and verification, and persistent graph storage.
- **`eontree-jury`** — the Global Sentinel filter and the weighted multi-judge evaluation engine described above.
- **`eontree-api`** — a REST API for browsing the graph, submitting new nodes, and listing the open counter-argument queue.
- **`eontree-cli`** — a command-line tool for authoring, signing, and submitting nodes, and for claiming counter-argument stubs, without needing to touch the API directly.

See each crate's own README for implementation details, and the top-level [README.md](README.md) for build and run instructions if you want to stand up a local instance.

## The Vision

EonTree is trying to build something like a trusted, continuously self-correcting reference for civilization's hardest problems — a place where the acceptance standards themselves are just another node in the graph, open to challenge and revision by the same process that vets everything else. If the community decides the bar is set wrong, that's a proposal like any other, not a decree from the maintainers.

Contributions, critiques, and claimed counter-arguments are welcome — that's the point.
