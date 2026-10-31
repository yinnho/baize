# T1 收工检查清单（规程）

1. 本轮是否成功？否 → stop。
2. steps≥2 或 tools≥2？否 → stop（一次性不固化）。
3. `capability_t1_review` → 若 flow_update → 改已有面；若 propose_* 且闸开 → `capability_propose(trigger=T1)`。
4. 默认 ≤ L1；禁止擅自 L2/L3 / 写名册。
