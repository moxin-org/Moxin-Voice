## ADDED Requirements

### Requirement: Voice profile SHALL support controlled gender-age categories
系统 MUST 为每个音色提供 `gender_age` 分类字段，且仅允许 `male`、`female`、`child` 三个枚举值。

#### Scenario: Create voice with valid gender-age value
- **WHEN** 用户创建或编辑音色并提交 `gender_age=male`
- **THEN** 系统保存成功并在后续读取中返回 `male`

#### Scenario: Reject invalid gender-age value
- **WHEN** 提交 `gender_age=elderly_male` 等非枚举值
- **THEN** 系统 MUST 拒绝写入并返回明确的校验错误

### Requirement: Voice profile SHALL support controlled style categories
系统 MUST 为每个音色提供 `style` 分类字段，且仅允许 `sweet`、`magnetic`、`youth` 三个枚举值。

#### Scenario: Create voice with valid style value
- **WHEN** 用户创建或编辑音色并提交 `style=magnetic`
- **THEN** 系统保存成功并在后续读取中返回 `magnetic`

#### Scenario: Reject invalid style value
- **WHEN** 提交 `style=warm` 等非枚举值
- **THEN** 系统 MUST 拒绝写入并返回明确的校验错误

### Requirement: System SHALL provide backward compatibility for missing tags
对于历史音色数据中缺失 `gender_age` 或 `style` 的记录，系统 MUST 在读取时提供兼容返回（默认值或未分类标识），并允许后续编辑后写回新结构。

#### Scenario: Read legacy voice without tags
- **WHEN** 读取一条未包含 `gender_age` 与 `style` 的历史音色记录
- **THEN** 系统返回可展示的兼容标签结果且不报错

#### Scenario: Save legacy voice after editing
- **WHEN** 用户编辑历史音色并保存
- **THEN** 系统将 `gender_age` 与 `style` 字段按新结构持久化
