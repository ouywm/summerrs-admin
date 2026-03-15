import request from '@/utils/http'

/**
 * 登录
 * @param params 登录参数
 * @returns 登录响应
 */
export function fetchLogin(params: Api.Auth.LoginParams) {
  return request.post<Api.Auth.LoginResponse>({
    url: '/api/auth/login',
    params
  })
}

/**
 * 刷新 Token
 * @param refreshToken 刷新令牌
 * @returns 新的登录响应（包含新的 accessToken 和 refreshToken）
 */
export function fetchRefreshToken(refreshToken: string) {
  return request.post<Api.Auth.LoginResponse>({
    url: '/api/auth/refresh',
    params: { refreshToken }
  })
}

/**
 * 获取用户信息
 * @returns 用户信息
 */
export function fetchGetUserInfo() {
  return request.get<Api.Auth.UserInfo>({
    url: '/api/user/info'
  })
}

/** 退出登录 */
export function fetchLogout() {
  return request.post<null>({
    url: '/api/auth/logout'
  })
}
