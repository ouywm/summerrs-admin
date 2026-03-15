import request from '@/utils/http'

/** 获取登录日志列表（管理员） */
export function fetchGetLoginLogList(params: Api.LoginLog.LoginLogSearchParams) {
  return request.get<Api.LoginLog.LoginLogList>({
    url: '/api/login-log/list',
    params
  })
}

/** 获取当前用户的登录日志列表 */
export function fetchGetMyLoginLogList(params: Api.LoginLog.LoginLogSearchParams) {
  return request.get<Api.LoginLog.LoginLogList>({
    url: '/api/user/profile/login-logs',
    params
  })
}
