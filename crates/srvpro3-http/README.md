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
