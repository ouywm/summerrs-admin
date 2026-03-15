/**
 * API 接口全局类型定义
 *
 * 通用类型（分页参数、响应结构等），各模块类型拆分至独立文件：
 * - auth.d.ts — 认证类型（登录、用户信息等）
 * - system.d.ts — 系统管理类型（用户、角色等）
 *
 * ## 注意事项
 *
 * - 在 .vue 文件使用需要在 eslint.config.mjs 中配置 globals: { Api: 'readonly' }
 * - 使用全局命名空间，无需导入即可使用
 * - 同一 namespace 可分散在多个 .d.ts 文件中，TypeScript 会自动合并
 *
 * @module types/api/api
 * @author Art Design Pro Team
 */

declare namespace Api {
  /** 通用类型 */
  namespace Common {
    /** 分页参数 */
    interface PaginationParams {
      /** 当前页码 */
      page: number
      /** 每页条数 */
      size: number
      /** 总条数 */
      total: number
    }

    /** 通用搜索参数 */
    type CommonSearchParams = Pick<PaginationParams, 'page' | 'size'>

    /** 分页响应基础结构 */
    interface PaginatedResponse<T = any> {
      /** 数据列表 */
      content: T[]
      /** 每页条数 */
      size: number
      /** 当前页码 */
      page: number
      /** 总元素数 */
      totalElements: number
      /** 总页数 */
      totalPages: number
    }

    /** 启用状态 */
    type EnableStatus = '1' | '2'
  }
}
