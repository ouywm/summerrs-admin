declare namespace Api {
  /** 操作日志类型 */
  namespace OperationLog {
    /** 业务操作类型（0=其他, 1=新增, 2=修改, 3=删除, 4=查询, 5=导出, 6=导入, 7=认证） */
    type BusinessType = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7

    /** 操作状态（1=成功, 2=失败, 3=异常） */
    type OperationStatus = 1 | 2 | 3

    /** 操作日志列表 */
    type OperationLogList = Api.Common.PaginatedResponse<OperationLogListItem>

    /** 操作日志列表项（对应后端 OperationLogVo） */
    interface OperationLogListItem {
      id: number
      /** 操作用户名 */
      userName: string
      /** 操作模块 */
      module: string
      /** 操作动作 */
      action: string
      /** 业务类型 */
      businessType: BusinessType
      /** 业务类型文本（后端返回：新增/修改/删除等） */
      businessTypeText: string
      /** 请求方式 */
      requestMethod: string
      /** 客户端IP */
      clientIp: string
      /** IP归属地 */
      ipLocation: string
      /** 操作状态 */
      status: OperationStatus
      /** 状态文本（后端返回：成功/失败/异常） */
      statusText: string
      /** 耗时（毫秒） */
      duration: number
      /** 创建时间 */
      createTime: string
    }

    /** 操作日志详情（对应后端 OperationLogDetailVo） */
    interface OperationLogDetail extends OperationLogListItem {
      /** 用户ID */
      userId: number
      /** 请求URL */
      requestUrl: string
      /** 请求参数（JSON字符串） */
      requestParams: string
      /** 响应结果（JSON字符串） */
      responseBody: string
      /** 响应状态码 */
      responseCode: number
      /** 浏览器User-Agent */
      userAgent: string
      /** 错误信息 */
      errorMsg: string
    }

    /** 操作日志查询参数（对应后端 OperationLogQueryDto） */
    interface OperationLogSearchParams extends Api.Common.CommonSearchParams {
      userName?: string
      module?: string
      action?: string
      businessType?: BusinessType
      requestMethod?: string
      requestUrl?: string
      clientIp?: string
      responseCode?: number
      status?: OperationStatus
      startTime?: string
      endTime?: string
    }
  }
}
