import request from '@/utils/http'

/** 获取操作日志列表 */
export function fetchGetOperationLogList(params: Api.OperationLog.OperationLogSearchParams) {
  return request.get<Api.OperationLog.OperationLogList>({
    url: '/api/operation-log/list',
    params
  })
}

/** 获取操作日志详情 */
export function fetchGetOperationLogDetail(id: number) {
  return request.get<Api.OperationLog.OperationLogDetail>({
    url: `/api/operation-log/${id}`
  })
}
