## ADDED Requirements

### Requirement: Frontend SHALL expose output timbre options
系统 MUST 在前端语音设置区域展示“输出音色”配置，至少包含语速和音调两个可选择维度，并让用户可见当前已选项。

#### Scenario: User views timbre settings
- **WHEN** 用户打开语音设置区域
- **THEN** 页面显示语速与音调的可选项，并明确标识当前选中值

### Requirement: Frontend SHALL provide bounded selectable values
系统 MUST 为语速与音调提供受控选项集合，用户只能选择预定义档位，且每个维度均存在默认值。

#### Scenario: User changes speed and pitch
- **WHEN** 用户在语速或音调选项中切换档位
- **THEN** 系统仅接受预定义值并立即更新该维度的选中态

#### Scenario: User has no prior selection
- **WHEN** 用户首次进入页面或历史状态缺失
- **THEN** 系统自动应用默认语速与默认音调

### Requirement: Selected timbre options SHALL be mapped into synthesis requests
系统 MUST 在发起语音合成请求时将已选语速与音调映射为请求参数，并确保每次请求都携带有效值。

#### Scenario: User submits synthesis request with selected options
- **WHEN** 用户在选择语速与音调后触发语音生成
- **THEN** 请求参数包含与所选档位对应的语速与音调值

#### Scenario: UI state is invalid or missing at request time
- **WHEN** 语音请求发起时发现语速或音调值为空或越界
- **THEN** 系统回退到默认映射值并继续发起有效请求
