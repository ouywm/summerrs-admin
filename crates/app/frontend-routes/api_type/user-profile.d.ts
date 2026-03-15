declare namespace Api {
  /** 用户个人中心类型 */
  namespace UserProfile {
    /** 用户个人信息 */
    interface UserProfileVo {
      userId: number
      userName: string
      nickName: string
      email: string
      phone: string
      gender: Api.SystemManage.Gender
      avatar: string
      updateTime: string
    }

    /** 修改密码参数 */
    interface ChangePasswordParams {
      oldPassword: string
      newPassword: string
    }

    /** 更新个人信息参数 */
    interface UpdateProfileParams {
      nickName?: string
      email?: string
      phone?: string
      gender?: Api.SystemManage.Gender
      avatar?: string
    }

    /** 登录日志列表 */
    type LoginLogList = Api.Common.PaginatedResponse<LoginLogVo>

    /** 登录日志项 */
    interface LoginLogVo {
      id: number
      userId: number
      userName: string
      loginTime: string
      loginIp: string
      loginLocation: string
      userAgent: string
      browser: string
      os: string
      device: string
      status: LoginStatus
      statusText: string
      failReason?: string
    }

    /** 登录状态 */
    type LoginStatus = 1 | 2 // 1=成功, 2=失败

    /** 登录日志查询参数 */
    interface LoginLogQueryParams extends Api.Common.CommonSearchParams {
      startTime?: string
      endTime?: string
      status?: LoginStatus
    }
  }
}
