declare namespace Api {
  /** 登录日志类型 */
  namespace LoginLog {
    /** 登录状态（1=成功, 2=失败） */
    type LoginStatus = 1 | 2

    /** 登录日志列表 */
    type LoginLogList = Api.Common.PaginatedResponse<LoginLogListItem>

    /** 登录日志列表项（对应后端 LoginLogVo） */
    interface LoginLogListItem {
      id: number
      /** 用户ID */
      userId: number
      /** 用户名 */
      userName: string
      /** 登录时间 */
      loginTime: string
      /** 登录IP */
      loginIp: string
      /** 登录地理位置 */
      loginLocation: string
      /** 浏览器User-Agent */
      userAgent: string
      /** 浏览器 */
      browser: string
      /** 浏览器版本 */
      browserVersion: string
      /** 操作系统 */
      os: string
      /** 操作系统版本 */
      osVersion: string
      /** 设备类型 */
      device: string
      /** 登录状态（1=成功, 2=失败） */
      status: LoginStatus
      /** 状态文本（后端返回：成功/失败） */
      statusText: string
      /** 失败原因 */
      failReason: string
    }

    /** 登录日志查询参数（对应后端 LoginLogQueryDto） */
    interface LoginLogSearchParams extends Api.Common.CommonSearchParams {
      userName?: string
      loginIp?: string
      status?: LoginStatus
      startTime?: string
      endTime?: string
    }
  }
}
