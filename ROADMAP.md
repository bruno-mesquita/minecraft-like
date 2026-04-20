# ROADMAP.md

## Status Atual

- [x] Workspace Rust organizado em `app`, `core`, `world`, `render` e `sim`
- [x] Geração procedural básica de chunks
- [x] Persistência binária local por chunk/região
- [x] Simulação first-person com colisão voxel/AABB
- [x] Janela com `winit`
- [x] Renderização básica com `wgpu`
- [x] Navegação em primeira pessoa com mouse e teclado

## Próximos Passos

### 1. Estabilizar o Runtime Jogável

- [ ] Melhorar spawn inicial do player para sempre nascer em local válido
- [ ] Corrigir sincronização de mesh/chunk para evitar trabalho redundante
- [ ] Refinar grounded, pulo e colisão vertical/lateral
- [ ] Adicionar delta time, frame time e contadores na tela ou no log
- [ ] Lidar melhor com erros de backend gráfico e fallback de plataforma

### 2. Melhorar o Pipeline de Mundo

- [ ] Separar claramente chunks `requested`, `generated`, `meshed`, `resident` e `evicted`
- [ ] Evitar rebuild de mesh para chunks não alterados
- [ ] Adicionar dirty flags específicas para voxel data e mesh data
- [ ] Processar geração e meshing em jobs paralelos
- [ ] Implementar eviction com save automático antes de descarregar

### 3. Evoluir a Renderização

- [ ] Trocar geração face-a-face por meshing mais eficiente
- [ ] Implementar greedy meshing
- [ ] Adicionar frustum culling por chunk
- [ ] Melhorar formato de vértice e upload de buffers
- [ ] Preparar suporte a atlas de textura
- [ ] Adicionar neblina/distância para esconder pop-in

### 4. Interação de Jogo

- [ ] Adicionar raycast do player para selecionar bloco
- [ ] Implementar quebrar bloco
- [ ] Implementar colocar bloco
- [ ] Adicionar hotbar mínima com poucos blocos
- [ ] Salvar alterações feitas pelo player no mundo

### 5. Geração de Terreno

- [ ] Melhorar heightmap com ruído mais natural
- [ ] Adicionar camadas de solo, pedra e biomas simples
- [ ] Gerar cavernas
- [ ] Gerar árvores e detalhes de superfície
- [ ] Separar geração por etapas para facilitar tuning

### 6. Ferramentas e Performance

- [ ] Criar benchmarks para geração, meshing e upload
- [ ] Adicionar profiling com `tracing` e `tracy`
- [ ] Medir uso de memória por chunk e por mesh
- [ ] Adicionar smoke test de inicialização do app
- [ ] Configurar CI para `cargo test` e `cargo check`

### 7. Base para Expansão

- [ ] Definir formato de save mais estável por região
- [ ] Adicionar configuração externa de engine/render/simulação
- [ ] Preparar sistema de entidades dinâmicas sem misturar chunk data no ECS
- [ ] Planejar iluminação simples
- [ ] Planejar arquitetura de multiplayer sem impactar o single-player atual

## Critério de Prioridade

1. Corrigir gargalos de runtime e inconsistências de simulação
2. Melhorar throughput de chunk streaming e meshing
3. Adicionar interação básica com blocos
4. Expandir geração e fidelidade visual
