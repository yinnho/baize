# L2 共享流程库检查清单（规程）

1. 闸开？否 → 零落盘。
2. `capability_propose(tier=L2)` 或 `capability_materialize_l2` / `capability_accept`。
3. 验收：`workflows/<名>/AGENTS.md` + `flows/` + `knowledge/` 在。
4. `workflows.md` **无**新行；无 gateway TOML；回执含「不可假定 agent://」。
5. 勿把 L2 当 `agent://` 直寻址；升 L3 走 P5。
