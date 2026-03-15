declare namespace Api {
  /** 系统管理类型 */
  namespace SystemManage {
    /** 性别（对应后端 Gender 枚举：0=未知, 1=男, 2=女） */
    type Gender = 0 | 1 | 2

    /** 用户状态（对应后端 UserStatus 枚举：1=启用, 2=禁用, 3=注销） */
    type UserStatus = 1 | 2 | 3

    /** 用户列表 */
    type UserList = Api.Common.PaginatedResponse<UserListItem>

    /** 用户列表项（对应后端 UserVo） */
    interface UserListItem {
      id: number
      avatar: string
      status: UserStatus
      userName: string
      userGender: string // 后端返回字符串 "男"/"女"/"未知"
      nickName: string
      userPhone: string
      userEmail: string
      userRoles: string[]
      createBy: string
      createTime: string
      updateBy: string
      updateTime: string
    }

    /** 角色详情（对应后端 RoleDetailVo） */
    interface RoleDetailVo {
      roleId: number
      roleName: string
      roleCode: string
    }

    /** 用户详情（对应后端 UserDetailVo） */
    interface UserDetailVo extends UserListItem {
      roles: RoleDetailVo[]
    }

    /** 用户查询参数（对应后端 UserQueryDto） */
    interface UserSearchParams extends Api.Common.CommonSearchParams {
      userName?: string
      phone?: string
      email?: string
      status?: UserStatus
      gender?: Gender
    }

    /** 创建用户参数（对应后端 CreateUserDto） */
    interface CreateUserParams {
      userName: string
      nickName: string
      gender?: Gender
      phone?: string
      email?: string
      avatar?: string
      status?: UserStatus
      roleIds?: number[]
    }

    /** 更新用户参数（对应后端 UpdateUserDto） */
    interface UpdateUserParams {
      nickName?: string
      gender?: Gender
      phone?: string
      email?: string
      avatar?: string
      status?: UserStatus
      roleIds?: number[]
    }

    /** 重置密码参数（对应后端 ResetPasswordDto） */
    interface ResetPasswordParams {
      newPassword: string
    }

    /** 更新用户状态参数（对应后端 UpdateUserStatusDto） */
    interface UpdateUserStatusParams {
      status: UserStatus
    }

    /** 角色列表 */
    type RoleList = Api.Common.PaginatedResponse<RoleVo>

    /** 角色列表项（对应后端 RoleVo） */
    interface RoleVo {
      roleId: number
      roleName: string
      roleCode: string
      description: string
      enabled: boolean
      createTime: string
    }

    /** 角色列表项别名 */
    type RoleListItem = RoleVo

    /** 角色搜索参数（对应后端 RoleQueryDto） */
    interface RoleSearchParams extends Api.Common.CommonSearchParams {
      roleName?: string
      roleCode?: string
      description?: string
      enabled?: boolean
      startTime?: string
      endTime?: string
    }

    /** 创建角色参数（对应后端 CreateRoleDto） */
    interface CreateRoleParams {
      roleName: string
      roleCode: string
      description?: string
      enabled?: boolean
    }

    /** 更新角色参数（对应后端 UpdateRoleDto） */
    interface UpdateRoleParams {
      roleName?: string
      description?: string
      enabled?: boolean
    }

    /** 角色权限（对应后端 RolePermissionVo） */
    interface RolePermissionVo {
      checkedKeys: number[]
      halfCheckedKeys: number[]
    }

    /** 角色权限参数（对应后端 RolePermissionDto） */
    interface RolePermissionParams {
      menuIds: number[]
    }

    /** 菜单类型 */
    type MenuType = 1 | 2 // 1-菜单 2-按钮权限

    /** 按钮权限项（对应后端 AuthItem） */
    interface AuthItem {
      id?: number
      parentId?: number
      title: string
      authName?: string
      authMark: string
      sort?: number
      enabled?: boolean
      createTime?: string
      updateTime?: string
    }

    /** 菜单元数据（对应后端 MenuMeta） */
    interface MenuMeta {
      title: string
      icon: string
      isHide: boolean
      isHideTab: boolean
      link: string
      isIframe: boolean
      keepAlive: boolean
      roles: string[]
      isFirstLevel: boolean
      fixedTab: boolean
      activePath: string
      isFullPage: boolean
      showBadge: boolean
      showTextBadge: string
      sort: number
      enabled: boolean
      authList: AuthItem[]
    }

    /** 菜单树（前端路由结构，对应后端 MenuTreeVo） */
    interface MenuTreeVo {
      id?: number
      parentId?: number
      path: string
      name: string
      component: string
      redirect: string
      meta: MenuMeta
      children: MenuTreeVo[]
    }

    /** 菜单列表项（对应后端 MenuVo） */
    interface MenuVo {
      id: number
      parentId: number
      menuType: MenuType
      name: string
      path: string
      component: string
      redirect: string
      icon: string
      title: string
      link: string
      isIframe: boolean
      isHide: boolean
      isHideTab: boolean
      isFullPage: boolean
      isFirstLevel: boolean
      keepAlive: boolean
      fixedTab: boolean
      showBadge: boolean
      showTextBadge: string
      activePath: string
      authName: string
      authMark: string
      sort: number
      enabled: boolean
      createTime: string
      updateTime: string
    }

    /** 创建菜单参数（对应后端 CreateMenuDto，menuType = 1） */
    interface CreateMenuParams {
      parentId?: number
      name: string
      path: string
      component?: string
      redirect?: string
      icon?: string
      title: string
      link?: string
      isIframe?: boolean
      isHide?: boolean
      isHideTab?: boolean
      isFullPage?: boolean
      isFirstLevel?: boolean
      keepAlive?: boolean
      fixedTab?: boolean
      showBadge?: boolean
      showTextBadge?: string
      activePath?: string
      sort?: number
      enabled?: boolean
    }

    /** 创建按钮参数（对应后端 CreateButtonDto，menuType = 2） */
    interface CreateButtonParams {
      parentId: number
      authName: string
      authMark: string
      sort?: number
      enabled?: boolean
    }

    /** 更新菜单参数（对应后端 UpdateMenuDto） */
    interface UpdateMenuParams {
      parentId?: number
      name?: string
      path?: string
      component?: string
      redirect?: string
      icon?: string
      title?: string
      link?: string
      isIframe?: boolean
      isHide?: boolean
      isHideTab?: boolean
      isFullPage?: boolean
      isFirstLevel?: boolean
      keepAlive?: boolean
      fixedTab?: boolean
      showBadge?: boolean
      showTextBadge?: string
      activePath?: string
      sort?: number
      enabled?: boolean
    }

    /** 更新按钮参数（对应后端 UpdateButtonDto） */
    interface UpdateButtonParams {
      parentId?: number
      authName?: string
      authMark?: string
      sort?: number
      enabled?: boolean
    }

    /** 菜单对话框表单数据（合并菜单和按钮的所有字段） */
    type MenuFormData = { menuType: MenuType } & Partial<CreateMenuParams> &
      Partial<CreateButtonParams>

    // ============================================================
    // 字典管理类型
    // ============================================================

    /** 字典状态（对应后端 DictStatus 枚举：1=启用, 2=禁用） */
    type DictStatus = 1 | 2

    /** 字典类型列表 */
    type DictTypeList = Api.Common.PaginatedResponse<DictTypeVo>

    /** 字典类型列表项（对应后端 DictTypeVo） */
    interface DictTypeVo {
      id: number
      dictName: string
      dictType: string
      status: DictStatus
      isSystem: boolean
      remark: string
      createBy: string
      createTime: string
      updateBy: string
      updateTime: string
    }

    /** 字典类型查询参数（对应后端 DictTypeQueryDto） */
    interface DictTypeSearchParams extends Api.Common.CommonSearchParams {
      dictName?: string
      dictType?: string
      status?: DictStatus
    }

    /** 创建字典类型参数（对应后端 CreateDictTypeDto） */
    interface CreateDictTypeParams {
      dictName: string
      dictType: string
      status?: DictStatus
      remark?: string
    }

    /** 更新字典类型参数（对应后端 UpdateDictTypeDto） */
    interface UpdateDictTypeParams {
      dictName?: string
      status?: DictStatus
      remark?: string
    }

    /** 字典数据列表 */
    type DictDataList = Api.Common.PaginatedResponse<DictDataVo>

    /** 字典数据列表项（对应后端 DictDataVo） */
    interface DictDataVo {
      id: number
      dictType: string
      dictLabel: string
      dictValue: string
      dictSort: number
      cssClass: string
      listClass: string
      isDefault: boolean
      status: DictStatus
      isSystem: boolean
      remark: string
      createBy: string
      createTime: string
      updateBy: string
      updateTime: string
    }

    /** 简化的字典数据（对应后端 DictDataSimpleVo，用于前端下拉框） */
    interface DictDataSimpleVo {
      label: string
      value: string
      listClass: string
    }

    /** 字典数据查询参数（对应后端 DictDataQueryDto） */
    interface DictDataSearchParams extends Api.Common.CommonSearchParams {
      dictType?: string
      dictLabel?: string
      status?: DictStatus
    }

    /** 创建字典数据参数（对应后端 CreateDictDataDto） */
    interface CreateDictDataParams {
      dictType: string
      dictLabel: string
      dictValue: string
      dictSort?: number
      cssClass?: string
      listClass?: string
      isDefault?: boolean
      status?: DictStatus
      remark?: string
    }

    /** 更新字典数据参数（对应后端 UpdateDictDataDto） */
    interface UpdateDictDataParams {
      dictLabel?: string
      dictValue?: string
      dictSort?: number
      cssClass?: string
      listClass?: string
      isDefault?: boolean
      status?: DictStatus
      remark?: string
    }
  }
}
