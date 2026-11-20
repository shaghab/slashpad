# ADR-0005: Normal / Large / Extreme document modes

Status: Proposed · Date: 2026-11-20

## Decision
One engine, three policies chosen at open (thresholds are settings, tuned in Milestone 4):

| | Normal | Large | Extreme |
|---|---|---|---|
| Default trigger | < 32 MB | 32 MB – 2 GB | ≥ 2 GB or any line > 16 MB |
| Original source | in memory | chunk cache | chunk cache |
| Line index | eager | background | background, sparse |
| Undo | unbounded (memory-capped) | bounded (e.g. 1 000 groups / 64 MB) | bounded |
| Highlighting | on | off by default | off |
| Editing | yes | yes | read-only by default, opt-in |

The user can switch mode for a document where safe (e.g. force Normal on a 100 MB file).
