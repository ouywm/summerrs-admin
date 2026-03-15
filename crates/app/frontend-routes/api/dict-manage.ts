import request from '@/utils/http'

// ============================================================
// 字典类型 API
// ============================================================

/** 获取字典类型列表 */
export function fetchGetDictTypeList(params: Api.SystemManage.DictTypeSearchParams) {
  return request.get<Api.SystemManage.DictTypeList>({
    url: '/api/dict/type/list',
    params
  })
}

/** 创建字典类型 */
export function fetchCreateDictType(params: Api.SystemManage.CreateDictTypeParams) {
  return request.post<null>({
    url: '/api/dict/type',
    params
  })
}

/** 更新字典类型 */
export function fetchUpdateDictType(id: number, params: Api.SystemManage.UpdateDictTypeParams) {
  return request.put<null>({
    url: `/api/dict/type/${id}`,
    params
  })
}

/** 删除字典类型 */
export function fetchDeleteDictType(id: number) {
  return request.del<null>({
    url: `/api/dict/type/${id}`
  })
}

// ============================================================
// 字典数据 API
// ============================================================

/** 获取字典数据列表 */
export function fetchGetDictDataList(params: Api.SystemManage.DictDataSearchParams) {
  return request.get<Api.SystemManage.DictDataList>({
    url: '/api/dict/data/list',
    params
  })
}

/** 根据类型获取字典数据（用于下拉框） */
export function fetchGetDictDataByType(dictType: string) {
  return request.get<Api.SystemManage.DictDataSimpleVo[]>({
    url: `/api/dict/data/by-type/${dictType}`
  })
}

/** 获取全量字典数据（返回所有字典类型及其数据） */
export function fetchGetAllDictData() {
  return request.get<Record<string, Api.SystemManage.DictDataSimpleVo[]>>({
    url: '/api/dict/all'
  })
}

/** 创建字典数据 */
export function fetchCreateDictData(params: Api.SystemManage.CreateDictDataParams) {
  return request.post<null>({
    url: '/api/dict/data',
    params
  })
}

/** 更新字典数据 */
export function fetchUpdateDictData(id: number, params: Api.SystemManage.UpdateDictDataParams) {
  return request.put<null>({
    url: `/api/dict/data/${id}`,
    params
  })
}

/** 删除字典数据 */
export function fetchDeleteDictData(id: number) {
  return request.del<null>({
    url: `/api/dict/data/${id}`
  })
}
