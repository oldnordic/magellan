# Utility-Aware Task Composition Research

**Date:** 2026-02-11

**Source:** https://multiagents.org/2026_papers/utility_aware_task_decomposition.pdf

## Overview

This paper focuses on **utility-aware task composition** - agents that cache intermediate results to avoid redundant computation when tasks are decomposed and recomposed.

## Key Findings

1. **Problem Domain**: Multi-agent systems doing redundant work
   - Same sub-computations repeated across different tasks
   - Need for task result memoization and sharing

2. **Solution**: Task Decomposition with Memoization
   - **Task Grammar**: Defines decomposition into atomic steps
   - **Memo Table**: Stores outputs from sub-tasks
   - **Utility Calculations**: Agents can request computed values from memo table
   - Ensures **determinism** and **efficiency** while maintaining correctness

3. **Relevant to magellan**:
   - Similar concept could optimize indexing by caching expensive queries (CFG extraction, AST traversals)
   - However, magellan's design is **command-based**, not continuously-running agents

4. **Not applicable directly** - This describes a specialized AI agent architecture for continuous operation

## Citation

```bibtex
@inproceedings{title={Utility-Aware Task Composition for Large Language Model Agents},
  author={Toby J. X. Li and Moshe Chesnut},
  booktitle={Proceedings of the Thirty-Ninth Annual Conference on Uncertainty in Artificial Intelligence},
  year={2026},
  editor={Michael Wooldridge and Ilia Zilberopoulos},
  publisher={AAAI Press},
  address={Seattle, Washington},
  month={November},
  pages={1378--1392},
  url={https://arxiv.org/html/2601.22290v3}
}
```

## Summary

This paper presents **Utility-Aware Task Composition** - a caching mechanism for sub-task results to enable reuse. Like the atomic task composition paper, this is **research on AI agent architecture**, not a magellan implementation guide.

### Action Required

1. ✅ Create documentation file in `/home/feanor/Projects/magellan/docs/research/` documenting this research
2. Create tasks for each CLI command to use atomic verification approach