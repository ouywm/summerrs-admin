import request from '@/utils/http'

/** 获取系统参数配置列表 */
export function fetchGetConfigList(
  params: Api.Config.ConfigSearchParams
) {
  return request.get<Api.Config.ConfigList>({
    url: '/api/config/list',
    params
  })
}

/** 获取系统参数配置详情 */
export function fetchGetConfigDetail(id: number) {
  return request.get<Api.Config.ConfigDetailVo>({
    url: `/api/config/${id}`
  })
}

/** 创建系统参数配置 */
export function fetchCreateConfig(
  params: Api.Config.CreateConfigParams
) {
  return request.post<null>({
    url: '/api/config',
    params
  })
}

/** 更新系统参数配置 */
export function fetchUpdateConfig(
  id: number,
  params: Api.Config.UpdateConfigParams
) {
  return request.put<null>({
    url: `/api/config/${id}`,
    params
  })
}

/** 删除系统参数配置 */
export function fetchDeleteConfig(id: number) {
  return request.del<null>({
    url: `/api/config/${id}`
  })
}
