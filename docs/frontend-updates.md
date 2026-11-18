# 前端文档、派生数据和求解结果更新

后端 reducer 仍是文档唯一写入者。GUI dispatch 在 reducer 改动内存文档后立即发送
`document-changed(revision)`，不用等待保存、兼容性修复和求解完成。dispatch 最终回执后
仍刷新一次，以收取后续修复和自动规划回写。MCP 的文档变更事件继续沿用同一入口。

前端 `refresh()` 合并并发刷新：已有请求返回时如还有更新的刷新要求，丢弃该快照并
补拉最新文档；所有调用者等待最新一轮结束。相同内容的快照不替换响应式文档，避免
重复创建列表派生对象及干扰当前编辑焦点。

普通工厂和宇宙工厂的求解缓存均按项目／工厂身份保存，输入变化时失效。
`SolveResult.revision` 是求解快照的版本，不是发送事件时的版本。前端按每个工厂
最后一次输入变化的版本拒绝旧结果，不因其他工厂的无关编辑拒绝有效结果。
普通工厂错误事件同样带项目、工厂和命令版本；保存失败只进入操作错误，避免冒充求解失败。

名称、行顺序、显示时间单位不使求解缓存失效；修改显示时间单位只保存，不重算。
重复选择当前工厂／项目不清焦点、不重算；切回正在求解的工厂不再次发起相同求解。

环境输入、里程碑、产能、可达性、历史状态及保存路径的异步响应均有代次保护。
项目派生数据通过快照中的实际输入变化刷新，因此本地编辑、撤销／重做、MCP 编辑走
同一条失效路径。仅修改目标、机制或外部输入价格不重新读取环境输入。

回归入口：

- `node --test tests/factory-signature.test.mjs`
- `npm run test:update-chain-browser`：主动延迟旧文档／派生数据请求，交错选择和编辑，验证
  实际 DOM、求解缓存隔离、版本拒绝与并发 busy 状态。
- `cargo test -p metatorio-runtime --no-default-features --features microlp,rust-lp --lib`
- `cargo test -p metatorio-shell --no-default-features --features microlp,rust-lp --lib -- --skip auto_plan_assets_tests`
- `npm run test:network-scope-switch-browser`：覆盖宇宙/普通作用域切换时的状态捕获与卸载销毁隔离（`MechanicNetworkView` 卸载只读捕获的本地状态，不触碰已切换后的父级派生链）。

仍有可优化的成本：新增机制后再指定原型、批量配置插件等复合操作，会发送多条独立
dispatch，每条数值修改仍各自求解。消除此类中间求解需要显式的批量／事务入口；
当前保留现有独立消息的保存和撤销语义。
