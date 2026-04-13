# File Center Entity Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update `summer-system` Rust entities + file business logic to match the redesigned SQL `sys.file` + new `sys.file_folder` tables (snake_case DB, camelCase API).

**Architecture:** Keep current router surface mostly stable, but remap DTO/VO + services to the new columns. Upload writes `sys.file` records using `object_key` as S3 key; listing/detail reads from `sys.file`; delete becomes soft-delete (`deleted_at`, `purge_status`) with async purge.

**Tech Stack:** Rust, SeaORM, Axum (summer-web), aws-sdk-s3.

---

### Task 1: Update SeaORM Entities For `sys.file` + `sys.file_folder`

**Files:**
- Create: `crates/summer-system/model/src/entity/sys_file_folder.rs`
- Modify: `crates/summer-system/model/src/entity/sys_file.rs`
- Modify: `crates/summer-system/model/src/entity/mod.rs`

- [ ] **Step 1: Run compile to confirm current baseline**

Run: `cargo test -p summer-system --tests`
Expected: PASS

- [ ] **Step 2: Implement `sys_file_folder` entity**

- [ ] **Step 3: Refactor `sys_file` entity to new columns**

- [ ] **Step 4: Run compile**

Run: `cargo test -p summer-system --tests`
Expected: PASS

---

### Task 2: Update DTO/VO For File Module

**Files:**
- Modify: `crates/summer-system/model/src/dto/sys_file.rs`
- Modify: `crates/summer-system/model/src/vo/sys_file.rs`

- [ ] **Step 1: Update list query DTO fields to new columns**
- [ ] **Step 2: Update upload/presign DTO field names (`object_key` etc)**
- [ ] **Step 3: Update VO shapes to include `file_no/object_key/visibility/status/...`**
- [ ] **Step 4: Run compile**

Run: `cargo test -p summer-system --tests`
Expected: PASS

---

### Task 3: Refactor Upload/Download Service To New Schema

**Files:**
- Modify: `crates/summer-system/src/service/sys_file_upload_service.rs`
- Modify: `crates/summer-plugins/src/s3/config.rs` (if needed; keep API stable if possible)

- [ ] **Step 1: Server-side upload inserts new `sys.file` record**
- [ ] **Step 2: Presigned callback inserts new `sys.file` record**
- [ ] **Step 3: Multipart complete inserts new `sys.file` record**
- [ ] **Step 4: Download uses `object_key/size/mime_type/original_name`**
- [ ] **Step 5: Run tests**

Run: `cargo test -p summer-system --tests`
Expected: PASS

---

### Task 4: Refactor File Management Service (List/Detail/Delete)

**Files:**
- Modify: `crates/summer-system/src/service/sys_file_service.rs`

- [ ] **Step 1: List/detail remap URL building to `object_key`**
- [ ] **Step 2: Delete becomes soft-delete + async purge**
- [ ] **Step 3: Run tests**

Run: `cargo test -p summer-system --tests`
Expected: PASS

---

### Task 5: Fix Cross-Module References To Old Columns

**Files:**
- Modify: `crates/summer-system/src/service/sys_user_service.rs`

- [ ] **Step 1: User delete cleanup updates `creator_id`/`deleted_by` instead of `upload_by_id`**
- [ ] **Step 2: Run tests**

Run: `cargo test -p summer-system --tests`
Expected: PASS

---

### Task 6: Add Minimal Unit Tests For New Helpers

**Files:**
- Modify: `crates/summer-common/src/file_util.rs`

- [ ] **Step 1: Write failing test for `generate_file_no()` format/uniqueness**
- [ ] **Step 2: Implement `generate_file_no()`**
- [ ] **Step 3: Run tests**

Run: `cargo test -p summer-common`
Expected: PASS

