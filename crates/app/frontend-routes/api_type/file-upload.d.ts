declare namespace Api {
  /** 文件上传类型 */
  namespace FileUpload {
    // ============================================================
    // 上传状态（前端引擎专用）
    // ============================================================

    /** 上传状态枚举 */
    type UploadStatus = 'idle' | 'computing_md5' | 'uploading' | 'paused' | 'completed' | 'error'

    // ============================================================
    // 文件响应类型（对应后端 VO）
    // ============================================================

    /** 文件上传成功响应（对应后端 FileUploadVo） */
    interface FileUploadVo {
      fileId: number
      originalName: string
      /** 文件访问 URL（公开桶直链） */
      url: string
      fileSize: number
    }

    /** 文件详情（对应后端 FileVo） */
    interface FileDetailVo {
      id: number
      fileName: string
      originalName: string
      filePath: string
      fileSize: number
      fileSuffix: string
      mimeType: string
      bucket: string
      /** 文件访问 URL（公开桶直链） */
      url: string
      uploadBy: string
      createTime: string
    }

    // ============================================================
    // 单文件 / 批量上传
    // ============================================================

    /** 批量上传响应（对应后端 BatchUploadVo） */
    interface BatchUploadResult {
      success: FileUploadVo[]
      failed: UploadFailureVo[]
    }

    /** 上传失败项（对应后端 UploadFailureVo） */
    interface UploadFailureVo {
      originalName: string
      reason: string
    }

    // ============================================================
    // Presigned 上传
    // ============================================================

    /** Presigned 上传请求参数（对应后端 PresignUploadDto） */
    interface PresignUploadParams {
      fileName: string
      fileSize: number
      /** 文件 MD5，传了触发秒传检查 */
      fileMd5?: string
    }

    /** Presigned 上传响应（对应后端 PresignedUploadVo） */
    interface PresignUploadResult {
      /** 是否秒传命中 */
      fastUploaded: boolean
      /** 秒传命中时返回文件信息 */
      file: FileUploadVo | null
      /** 预签名上传 URL（秒传命中时为 null） */
      uploadUrl: string | null
      /** 文件存储路径（秒传命中时为 null） */
      filePath: string | null
      /** URL 有效期秒数（秒传命中时为 null） */
      expiresIn: number | null
    }

    /** Presigned 上传回调参数（对应后端 PresignUploadCallbackDto） */
    interface PresignUploadCallbackParams {
      filePath: string
      originalName: string
      fileSize: number
      fileMd5?: string
    }

    /** Presigned 下载响应（对应后端 PresignedDownloadVo） */
    interface PresignDownloadResult {
      downloadUrl: string
      expiresIn: number
    }

    // ============================================================
    // 分片上传
    // ============================================================

    /** 分片上传初始化参数（对应后端 MultipartInitDto） */
    interface MultipartInitParams {
      fileName: string
      fileSize: number
      fileMd5: string
    }

    /** 分片上传初始化响应（对应后端 MultipartInitVo） */
    interface MultipartInitResult {
      fastUploaded: boolean
      file?: FileUploadVo
      uploadId?: string
      filePath?: string
      chunkSize?: number
      totalParts?: number
      partUrls?: PartUrl[]
      expiresIn?: number
    }

    /** 分片预签名 URL（对应后端 PartPresignedUrl） */
    interface PartUrl {
      partNumber: number
      uploadUrl: string
    }

    /** 查询已上传分片参数（对应后端 MultipartListPartsDto） */
    interface MultipartPartsParams {
      uploadId: string
      filePath: string
      fileSize: number
    }

    /** 已上传分片信息（对应后端 UploadedPartVo） */
    interface UploadedPart {
      partNumber: number
      eTag: string
      size: number
    }

    /** 查询已上传分片响应（对应后端 MultipartListPartsVo） */
    interface MultipartPartsResult {
      uploadedParts: UploadedPart[]
      pendingPartUrls: PartUrl[]
      expiresIn: number
    }

    /** 合并分片参数（对应后端 MultipartCompleteDto） */
    interface MultipartCompleteParams {
      uploadId: string
      filePath: string
      originalName: string
      fileSize: number
      fileMd5?: string
    }

    /** 取消分片上传参数（对应后端 MultipartAbortDto） */
    interface MultipartAbortParams {
      uploadId: string
      filePath: string
    }

    // ============================================================
    // 文件管理（列表/详情/删除）
    // ============================================================

    /** 文件列表查询参数（对应后端 FileQueryDto） */
    interface FileQueryParams {
      originalName?: string
      fileSuffix?: string
      bucket?: string
      uploadBy?: string
    }

    /** 文件列表（分页） */
    type FileList = Api.Common.PaginatedResponse<FileDetailVo>

    // ============================================================
    // Web Worker 消息类型
    // ============================================================

    /** Worker 请求消息 */
    type Md5WorkerRequest = { type: 'start'; file: File; chunkSize: number } | { type: 'cancel' }

    /** Worker 响应消息 */
    type Md5WorkerResponse =
      | { type: 'progress'; percent: number }
      | { type: 'complete'; md5: string }
      | { type: 'error'; message: string }
      | { type: 'cancelled' }

    // ============================================================
    // 上传引擎配置 & 回调（前端专用）
    // ============================================================

    /** 引擎配置 */
    interface UploadEngineConfig {
      /** 并发上传数，默认 3 */
      concurrency?: number
      /** 最大重试次数，默认 3 */
      maxRetries?: number
      /** 重试基础延迟（ms），默认 1000 */
      retryDelay?: number
    }

    /** 上传进度信息 */
    interface UploadProgress {
      /** 总进度百分比 0-100 */
      percent: number
      /** 已上传字节 */
      loaded: number
      /** 总字节 */
      total: number
      /** 上传速度（字节/秒） */
      speed: number
      /** MD5 计算进度百分比 0-100 */
      md5Percent: number
      /** 已完成分片数 */
      completedParts: number
      /** 总分片数 */
      totalParts: number
      /** 是否秒传命中 */
      fastUploaded: boolean
      /** MD5 计算耗时（毫秒） */
      md5Duration: number
      /** 上传耗时（毫秒），包含从开始上传到完成的总耗时 */
      uploadDuration: number
    }

    /** 引擎回调接口 */
    interface UploadEngineCallbacks {
      /** 进度更新 */
      onProgress?: (progress: UploadProgress) => void
      /** 上传完成 */
      onComplete?: (file: FileUploadVo) => void
      /** 上传出错 */
      onError?: (error: Error) => void
      /** 状态变更 */
      onStatusChange?: (status: UploadStatus) => void
    }
  }
}
