# AGENTS.md

Este repositório existe para construir um voxel sandbox em `Rust` com prioridade absoluta em `performance`, `frame time previsível` e `baixo consumo de memória`.

## Objetivo

Os agentes devem:

- implementar a base técnica do jogo com foco em throughput e previsibilidade
- manter a arquitetura orientada a dados desde o começo
- tratar render, streaming e chunk pipeline como hot paths críticos

Os agentes não devem:

- expandir o escopo para features de jogo não essenciais
- introduzir abstrações genéricas cedo demais
- trocar previsibilidade por ergonomia em código de hot path

## Escopo Inicial Permitido

- janela, input e loop principal
- renderização de chunks
- armazenamento de voxels e chunks
- geração procedural
- streaming e eviction de chunks
- save/load local em arquivos binários por região
- câmera, movimentação e colisão voxel
- métricas, logging e profiling

## Escopo Proibido Sem Autorização

- multiplayer
- scripting
- UI complexa
- crafting, inventário avançado, mobs ou IA
- física genérica
- colocar chunk/world data dentro do ECS
- dependências pesadas que escondam custo operacional

## Regras de Arquitetura

- `chunk` e `world` data ficam fora do ECS
- ECS leve serve apenas para entidades dinâmicas
- preferir dados contíguos e layouts cache-friendly
- evitar alocações por frame em caminhos quentes
- separar dados quentes de metadados frios
- jobs pesados rodam fora da thread principal
- o main thread integra resultados e faz submit de render
- toda abstração em hot path deve ser defendida por benchmark ou profiling

## Regras de Dependência

- crate nova só entra se resolver problema claro de performance, manutenção ou observabilidade
- preferir bibliotecas pequenas e composáveis
- evitar engines completas e camadas genéricas sobre `wgpu`

## Regras de Mudança

- mudanças pequenas e revisáveis
- se tocar em render, world, streaming ou save/load, medir impacto antes de expandir o escopo
- não misturar refactor estético com mudança funcional
- não reformatar arquivos fora do escopo da tarefa

## Métricas Obrigatórias

Cada subsistema crítico deve expor ou alimentar métricas para:

- tempo de geração de chunk
- tempo de meshing
- tempo de upload para GPU
- tempo de save/load
- contagem de chunks por estado
- orçamento de trabalho consumido por frame

## Regra Final

Em dúvida entre ergonomia e throughput no hot path, escolher throughput.

Em dúvida arquitetural, preservar simplicidade de dados, previsibilidade e facilidade de profiling.
