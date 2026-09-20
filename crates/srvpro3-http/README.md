# HTTP 鉴权

所有已注册接口统一验证 query 中的 `user` 和 `password`；`username` 是 `user` 的别名。参数需要按 URL 规则编码。

```text
/room/list?user=reader&password=你的密码&page=0&page_size=60
/history/list?user=reader&password=你的密码&page=0&page_size=60
```

账号来自 `config.http_api.user`。配置文件兼容 `[api]` 作为 `[http_api]` 的别名，但不能同时写两套：

```toml
[http_api.user]
reader = { password = "请替换为自己的密码", permissions = 2 }
```

权限为 `0` 无权限、`1` 写、`2` 读、`3` 管理员。当前列表接口要求读权限或管理员；写权限不包含读权限。缺失或错误凭据返回 401，凭据正确但权限不足返回 403。默认账号权限为 0，不会因密码为空而绕过鉴权。

query 密码可能出现在浏览器历史、代理或访问日志中。部署时请使用 HTTPS，并避免记录完整 URL。服务端不打印凭据，响应使用 `Cache-Control: no-store` 和 `Referrer-Policy: no-referrer`，但这些响应头无法清除已经产生的外部日志。

## 重载

`POST /reload?user=账号&password=密码`，要求写权限（1）或管理员（3），无需请求体。

与控制台输入 `reload` 共用完整重载流程：读取 `config.toml`，重载卡片、数据库、HTTP、对局服务器及 WindBot。完成后按最新的 `cards.reload`（秒）重新计时，小于等于 0 时禁用定时重载。定时重载仍只重载卡片。

完成后返回 `200` 和 `{"reloaded":true}`；失败返回 `500`，详细原因见服务器日志，已应用的配置不会自动回滚。命令循环未启动或停止时返回 `503`，等待队列已满时返回 `429`。修改 HTTP 端口后，后续请求需使用新端口。
