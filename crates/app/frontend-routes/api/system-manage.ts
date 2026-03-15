import request from '@/utils/http'
import { AppRouteRecord } from '@/types/router'

/** 获取用户列表 */
export function fetchGetUserList(params: Api.SystemManage.UserSearchParams) {
  return request.get<Api.SystemManage.UserList>({
    url: '/api/user/list',
    params
  })
}

/** 获取用户详情 */
export function fetchGetUserDetail(id: number) {
  return request.get<Api.SystemManage.UserDetailVo>({
    url: `/api/user/${id}`
  })
}

/** 创建用户 */
export function fetchCreateUser(params: Api.SystemManage.CreateUserParams) {
  return request.post<null>({
    url: '/api/user',
    params
  })
}

/** 更新用户 */
export function fetchUpdateUser(id: number, params: Api.SystemManage.UpdateUserParams) {
  return request.put<null>({
    url: `/api/user/${id}`,
    params
  })
}

/** 删除用户 */
export function fetchDeleteUser(id: number) {
  return request.del<null>({
    url: `/api/user/${id}`
  })
}

/** 重置用户密码 */
export function fetchResetUserPassword(id: number, params: Api.SystemManage.ResetPasswordParams) {
  return request.put<null>({
    url: `/api/user/${id}/reset-password`,
    params
  })
}

/** 更新用户状态 */
export function fetchUpdateUserStatus(id: number, params: Api.SystemManage.UpdateUserStatusParams) {
  return request.put<null>({
    url: `/api/user/${id}/status`,
    params
  })
}

/** 获取角色列表 */
export function fetchGetRoleList(params: Api.SystemManage.RoleSearchParams) {
  return request.get<Api.SystemManage.RoleList>({
    url: '/api/role/list',
    params
  })
}

/** 创建角色 */
export function fetchCreateRole(params: Api.SystemManage.CreateRoleParams) {
  return request.post<null>({
    url: '/api/role',
    params
  })
}

/** 更新角色 */
export function fetchUpdateRole(roleId: number, params: Api.SystemManage.UpdateRoleParams) {
  return request.put<null>({
    url: `/api/role/${roleId}`,
    params
  })
}

/** 删除角色 */
export function fetchDeleteRole(roleId: number) {
  return request.del<null>({
    url: `/api/role/${roleId}`
  })
}

/** 获取角色权限 */
export function fetchGetRolePermissions(roleId: number) {
  return request.get<Api.SystemManage.RolePermissionVo>({
    url: `/api/role/${roleId}/permissions`
  })
}

/** 保存角色权限 */
export function fetchSaveRolePermissions(
  roleId: number,
  params: Api.SystemManage.RolePermissionParams
) {
  return request.put<null>({
    url: `/api/role/${roleId}/permissions`,
    params
  })
}

/** 获取菜单列表 */
export function fetchGetMenuList() {
  return request.get<AppRouteRecord[]>({
    url: '/api/v3/system/menus'
  })
}

/** 获取所有菜单列表（管理用） - 返回树形结构 */
export function fetchGetAllMenuList() {
  return request.get<AppRouteRecord[]>({
    url: '/api/system/menu/list'
  })
}

/** 创建菜单 */
export function fetchCreateMenu(params: Api.SystemManage.CreateMenuParams) {
  return request.post<Api.SystemManage.MenuVo>({
    url: '/api/system/menu',
    params
  })
}

/** 创建按钮 */
export function fetchCreateButton(params: Api.SystemManage.CreateButtonParams) {
  return request.post<Api.SystemManage.MenuVo>({
    url: '/api/system/button',
    params
  })
}

/** 更新菜单 */
export function fetchUpdateMenu(id: number, params: Api.SystemManage.UpdateMenuParams) {
  return request.put<Api.SystemManage.MenuVo>({
    url: `/api/system/menu/${id}`,
    params
  })
}

/** 更新按钮 */
export function fetchUpdateButton(id: number, params: Api.SystemManage.UpdateButtonParams) {
  return request.put<Api.SystemManage.MenuVo>({
    url: `/api/system/button/${id}`,
    params
  })
}

/** 删除菜单 */
export function fetchDeleteMenu(id: number) {
  return request.del<null>({
    url: `/api/system/menu/${id}`
  })
}
