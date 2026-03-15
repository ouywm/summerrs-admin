import request from '@/utils/http'

/**
 * 修改个人密码
 * @param params 修改密码参数
 */
export function fetchChangePassword(params: Api.UserProfile.ChangePasswordParams) {
  return request.put<null>({
    url: '/api/user/profile/password',
    params
  })
}

/**
 * 更新个人信息
 * @param params 更新信息参数
 */
export function fetchUpdateProfile(params: Api.UserProfile.UpdateProfileParams) {
  return request.put<Api.UserProfile.UserProfileVo>({
    url: '/api/user/profile',
    params
  })
}

/**
 * 获取登录日志
 * @param params 查询参数
 */
export function fetchLoginLogs(params: Api.UserProfile.LoginLogQueryParams) {
  return request.get<Api.UserProfile.LoginLogList>({
    url: '/api/user/profile/login-logs',
    params
  })
}
