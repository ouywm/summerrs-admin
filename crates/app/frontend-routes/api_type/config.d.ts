declare namespace Api {
  /** 系统参数配置类型 */
  namespace Config {
    /** 系统参数配置列表 */
    type ConfigList = Api.Common.PaginatedResponse<ConfigVo>

    /** 系统参数配置列表项（对应后端 ConfigVo） */
    interface ConfigVo {
      /** 配置ID */
      id: number
      /** 配置名称 */
      configName: string
      /** 配置键 */
      configKey: string
      /** 当前配置值 */
      configValue: string
      /** 默认配置值 */
      defaultValue: string
      /** 值类型 */
      valueType: 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8
      /** 配置分组编码 */
      configGroup: string
      /** 候选项字典类型编码 */
      optionDictType: string
      /** 同分组内排序 */
      configSort: number
      /** 是否启用 */
      enabled: boolean
      /** 是否系统内置 */
      isSystem: boolean
      /** 备注 */
      remark: string
      /** 创建人 */
      createBy: string
      /** 创建时间 */
      createTime: string
      /** 更新人 */
      updateBy: string
      /** 更新时间 */
      updateTime: string
    }

    /** 系统参数配置详情 */
    type ConfigDetailVo = ConfigVo

    /** 系统参数配置查询参数（对应后端 ConfigQueryDto） */
    interface ConfigSearchParams extends Api.Common.CommonSearchParams {
      /** 配置ID */
      id?: number
      /** 配置名称 */
      configName?: string
      /** 配置键 */
      configKey?: string
      /** 当前配置值 */
      configValue?: string
      /** 默认配置值 */
      defaultValue?: string
      /** 值类型 */
      valueType?: 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8
      /** 配置分组编码 */
      configGroup?: string
      /** 候选项字典类型编码 */
      optionDictType?: string
      /** 同分组内排序 */
      configSort?: number
      /** 是否启用 */
      enabled?: boolean
      /** 是否系统内置 */
      isSystem?: boolean
      /** 备注 */
      remark?: string
      /** 创建人 */
      createBy?: string
      /** 创建时间 */
      createTime?: string
      /** 创建时间开始 */
      createTimeStart?: string
      /** 创建时间结束 */
      createTimeEnd?: string
      /** 更新人 */
      updateBy?: string
      /** 更新时间 */
      updateTime?: string
      /** 更新时间开始 */
      updateTimeStart?: string
      /** 更新时间结束 */
      updateTimeEnd?: string
    }

    /** 创建系统参数配置参数（对应后端 CreateConfigDto） */
    interface CreateConfigParams {
      /** 配置名称 */
      configName: string
      /** 配置键 */
      configKey: string
      /** 当前配置值 */
      configValue?: string
      /** 默认配置值 */
      defaultValue?: string
      /** 值类型 */
      valueType?: 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8
      /** 配置分组编码 */
      configGroup?: string
      /** 候选项字典类型编码 */
      optionDictType?: string
      /** 同分组内排序 */
      configSort?: number
      /** 是否启用 */
      enabled?: boolean
      /** 是否系统内置 */
      isSystem?: boolean
      /** 备注 */
      remark?: string
    }

    /** 更新系统参数配置参数（对应后端 UpdateConfigDto） */
    interface UpdateConfigParams {
      /** 配置名称 */
      configName?: string
      /** 配置键 */
      configKey?: string
      /** 当前配置值 */
      configValue?: string
      /** 默认配置值 */
      defaultValue?: string
      /** 值类型 */
      valueType?: 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8
      /** 配置分组编码 */
      configGroup?: string
      /** 候选项字典类型编码 */
      optionDictType?: string
      /** 同分组内排序 */
      configSort?: number
      /** 是否启用 */
      enabled?: boolean
      /** 是否系统内置 */
      isSystem?: boolean
      /** 备注 */
      remark?: string
    }
  }
}
