# Ábaco

Suíte de escritório de código aberto, começando pela planilha. O aplicativo
desktop usa [Tauri](https://tauri.app) e o motor de cálculo é o
[IronCalc](https://www.ironcalc.com), ambos em Rust.

![Ábaco com um relatório financeiro aberto de um arquivo .xlsx, com cores, negrito e valores em reais](docs/screenshot.png)

## Estado atual

Projeto no início. O que já funciona:

- Abrir e salvar arquivos `.xlsx` (Ctrl+O, Ctrl+S, Ctrl+Shift+S), com aviso
  antes de descartar alterações não salvas.
- Várias planilhas por arquivo, com abas e fórmulas entre planilhas.
- Grade desenhada em canvas, com seleção por mouse e teclado e rolagem.
- Estilos vindos do arquivo: negrito, itálico, sublinhado, tachado, cor do
  texto, cor de fundo, alinhamento e formatos numéricos.
- Edição pela barra de fórmulas (Enter, Tab e Esc).
- Desfazer e refazer (Ctrl+Z, Ctrl+Y).
- As fórmulas e funções do IronCalc, incluindo `LET`, `LAMBDA` e matrizes
  dinâmicas.

Limitações conhecidas:

- O IronCalc 0.9 ainda não tem português. Os números usam a localidade
  espanhola, que tem o mesmo formato do Brasil (`18.500,00`, `;` entre
  argumentos), e os nomes de função ficam em inglês (`SUM`, não `SOMA`).
  `R$` digitado numa célula ainda não é reconhecido como moeda.
- Os estilos são exibidos, mas ainda não dá para alterá-los pela interface.
- Larguras de coluna, alturas de linha, células mescladas, bordas e gráficos
  do arquivo ainda não aparecem na grade.
- Fechar a janela não avisa sobre alterações não salvas.
- Arquivos gerados pela biblioteca openpyxl mostram `0` em células vazias que
  só têm estilo. É um defeito do importador do IronCalc com células gravadas
  como `t="n"` sem valor; arquivos no formato que o Excel grava não têm o
  problema.

## Estrutura

| Pasta | Conteúdo |
| --- | --- |
| `apps/desktop/src` | Interface em TypeScript (Vite). |
| `apps/desktop/src-tauri` | Casca Tauri. `engine.rs` é a ponte com o IronCalc. |

A interface fala com o motor apenas pela interface `Engine`
(`apps/desktop/src/engine.ts`). Hoje ela chama comandos do Tauri; a versão web
vai implementar a mesma interface com o IronCalc compilado para WebAssembly.

## Como rodar

Pré-requisitos: [Rust](https://rustup.rs), Node.js 20 ou mais novo e as
[dependências de sistema do Tauri](https://tauri.app/start/prerequisites/).

```sh
cd apps/desktop
npm install
npm run tauri dev
```

`npm run dev` abre só a interface no navegador, sem o motor de cálculo, o que
serve para trabalhar no visual.

Testes, a partir da raiz:

```sh
cargo test
```

## Próximos passos

1. Localidade e nomes de função em português, no IronCalc.
2. Formatação pela interface (negrito, cores, formato numérico).
3. Larguras de coluna, alturas de linha e células mescladas.
4. Renomear, excluir e reordenar planilhas.
5. Versão web com o motor em WebAssembly.

## Licença

Ainda não definida.
