## ADDED Requirements

### Requirement: UI SHALL provide gender-age filter options
音色管理界面 MUST 提供男声、女声、童声筛选项，并映射到 `male`、`female`、`child` 标签值。

#### Scenario: Filter by one gender-age category
- **WHEN** 用户选择“女声”筛选项
- **THEN** 列表仅显示 `gender_age=female` 的音色记录

#### Scenario: Clear gender-age filter
- **WHEN** 用户清空性别年龄筛选
- **THEN** 列表恢复显示所有可见音色（受其他筛选条件影响）

### Requirement: UI SHALL provide style filter options
音色管理界面 MUST 提供甜美、磁性、青年音筛选项，并映射到 `sweet`、`magnetic`、`youth` 标签值。

#### Scenario: Filter by one style category
- **WHEN** 用户选择“磁性”筛选项
- **THEN** 列表仅显示 `style=magnetic` 的音色记录

#### Scenario: Clear style filter
- **WHEN** 用户清空风格筛选
- **THEN** 列表恢复显示所有可见音色（受其他筛选条件影响）

### Requirement: System SHALL support combined filtering across dimensions
系统 MUST 支持组合筛选：同一维度多选按 OR 匹配，不同维度之间按 AND 匹配。

#### Scenario: Cross-dimension intersection filtering
- **WHEN** 用户同时选择“男声”和“磁性”
- **THEN** 列表仅显示同时满足 `gender_age=male` 且 `style=magnetic` 的音色

#### Scenario: Multi-select within one dimension
- **WHEN** 用户在性别年龄维度同时选择“男声”和“童声”
- **THEN** 列表显示 `gender_age=male` 或 `gender_age=child` 的音色
