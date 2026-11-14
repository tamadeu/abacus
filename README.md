# Ábaco

Suíte de escritório de código aberto, começando pela planilha. O aplicativo
desktop usa [Tauri](https://tauri.app) e o motor de cálculo é o
[IronCalc](https://www.ironcalc.com), ambos em Rust.

![Ábaco com valores, fórmulas, uma matriz dinâmica e um erro de divisão por zero](docs/screenshot.png)

## Estado atual

Projeto no início. O que já funciona:

- Grade desenhada em canvas, com seleção por mouse e teclado e rolagem.
- Edição pela barra de fórmulas (Enter, Tab e Esc).
- Desfazer e refazer (Ctrl+Z, Ctrl+Y).
- As fórmulas e funções do IronCalc, incluindo `LET`, `LAMBDA` e matrizes
  dinâmicas.

Limitações conhecidas:

- O IronCalc 0.9 ainda não tem português. Os números usam a localidade
  espanhola, que tem o mesmo formato do Brasil (`18.500,00`, `;` entre
  argumentos), e os nomes de função ficam em inglês (`SUM`, não `SOMA`).
  `R$` ainda não é reconhecido como moeda.
- Ainda não há abrir nem salvar arquivos: a planilha vive só na memória.

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

1. Abrir e salvar `.xlsx` (o IronCalc já lê e grava o formato).
2. Localidade e nomes de função em português, no IronCalc.
3. Estilos de célula e formatos numéricos na interface.
4. Várias planilhas por pasta de trabalho.
5. Versão web com o motor em WebAssembly.

## Licença

Ainda não definida.
