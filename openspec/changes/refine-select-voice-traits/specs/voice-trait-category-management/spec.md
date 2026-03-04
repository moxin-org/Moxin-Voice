## ADDED Requirements

### Requirement: Voice Trait Categories Must Match Product Configuration
The system SHALL render `select voice` trait categories from the managed category configuration and MUST NOT display deprecated categories.

#### Scenario: Deprecated category is removed from UI
- **WHEN** user opens the `select voice` trait category list
- **THEN** the list does not contain `童声`

#### Scenario: New categories are visible
- **WHEN** user opens the `select voice` trait category list
- **THEN** the list contains `专业播音` and `特色人物`

### Requirement: Professional Broadcast Category Must Contain Required Voices
The system SHALL map `专业播音` to the exact required voice set: `白岩松`, `罗翔`, `沈逸`.

#### Scenario: Professional broadcast voices are complete
- **WHEN** user switches to `专业播音`
- **THEN** the available voices include `白岩松`, `罗翔`, and `沈逸`

#### Scenario: Professional broadcast voices exclude unrelated entries
- **WHEN** user switches to `专业播音`
- **THEN** voices outside its mapping are not shown in this category list

### Requirement: Featured Character Category Must Contain Required Voices
The system SHALL map `特色人物` to the exact required voice set: `马云`, `杨幂`, `周杰伦`, `trump罗翔`.

#### Scenario: Featured character voices are complete
- **WHEN** user switches to `特色人物`
- **THEN** the available voices include `马云`, `杨幂`, `周杰伦`, and `trump罗翔`

#### Scenario: Featured character voices exclude unrelated entries
- **WHEN** user switches to `特色人物`
- **THEN** voices outside its mapping are not shown in this category list

### Requirement: Invalid Persisted Trait Selection Must Fallback Safely
If a persisted trait value references a removed category (for example `童声`), the system MUST fallback to a valid default category during initialization.

#### Scenario: Persisted removed category is recovered
- **WHEN** app restores trait selection and the saved value is `童声`
- **THEN** system replaces it with a valid category and renders a valid voice list
