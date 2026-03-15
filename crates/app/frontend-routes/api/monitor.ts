import request from '@/utils/http'

// ─── 服务监控 ────────────────────────────────────────────────────────────────

/** 获取服务器信息 */
export function fetchGetServerInfo() {
  return request.get<Api.Monitor.ServerInfoVo>({
    url: '/api/monitor/server'
  })
}

// ─── 缓存监控 ────────────────────────────────────────────────────────────────

/** 获取缓存信息 */
export function fetchGetCacheInfo() {
  return request.get<Api.Monitor.CacheInfoVo>({
    url: '/api/monitor/cache/info'
  })
}

/** 获取缓存键列表 */
export function fetchGetCacheKeys(params?: Api.Monitor.CacheKeysQuery) {
  return request.get<Api.Monitor.CacheKeysVo>({
    url: '/api/monitor/cache/keys',
    params
  })
}

/** 获取缓存键详情 */
export function fetchGetCacheKeyDetail(key: string) {
  return request.get<Api.Monitor.CacheKeyDetailVo>({
    url: `/api/monitor/cache/keys/${encodeURIComponent(key)}/value`
  })
}

/** 删除缓存键 */
export function fetchDeleteCacheKey(key: string) {
  return request.del<null>({
    url: `/api/monitor/cache/keys/${encodeURIComponent(key)}`
  })
}

/** 批量删除缓存键（按模式匹配） */
export function fetchDeleteCacheKeysByPattern(params: Api.Monitor.CacheDeleteQuery) {
  return request.del<null>({
    url: '/api/monitor/cache/keys',
    params
  })
}
