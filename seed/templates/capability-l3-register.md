# L3 满 D17 检查清单（规程）

1. 已有 L2：`workflows/<名>/AGENTS.md`？否 → 先 materialize_l2。
2. 总闸开？人审/admin 已显式批准（`admin_approved`/`owner_approved`）？
3. `capability_register_l3`（工具 admin-gated）→ 复用 `gateway_registry::register`。
4. 三收据：名册行、agents toml、workflows 目录（list 含名）。
5. 降档：`capability_unregister_l3` 摘名册+toml，保留目录。
