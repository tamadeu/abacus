# Ábaco

Suíte de escritório de código aberto, começando pela planilha. O núcleo é
escrito em Rust e o aplicativo desktop usa [Tauri](https://tauri.app).

![Ábaco com valores, fórmulas e um erro de divisão por zero](docs/screenshot.png)

## Estado atual

Projeto no início. O que já funciona:

- Grade desenhada em canvas, com seleção por mouse e teclado e rolagem.
- Edição pela barra de fórmulas (Enter, Tab e Esc).
- Números no padrão brasileiro (`18.500,00`, `R$ 9.200,00`).
- Fórmulas com `+ - * / ^`, parênteses, referências (`=B3-C3`) e intervalos.
- Funções `SOMA`, `MÉDIA`, `MÁXIMO`, `MÍNIMO` e `CONT.NÚM`, com os nomes em
  inglês como sinônimos.
- Erros `#DIV/0!`, `#VALOR!`, `#NOME?` e detecção de referência circular.

Ainda não há leitura ou gravação de arquivos: a planilha vive só na memória.

## Estrutura

| Pasta | Conteúdo |
| --- | --- |
| `crates/abacus-core` | Modelo de dados: referências, valores, planilhas. Sem dependências. |
| `crates/abacus-formula` | Analisador sintático e avaliador de fórmulas. |
| `apps/desktop` | Interface em TypeScript (Vite) e casca Tauri em `src-tauri`. |

A interface fala com o motor apenas pela interface `Engine`
(`apps/desktop/src/engine.ts`). Hoje ela chama comandos do Tauri; a versão web
vai implementar a mesma interface com o núcleo compilado para WebAssembly.

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

Testes do núcleo, a partir da raiz:

```sh
cargo test
```

## Próximos passos

1. Leitura e gravação de `.xlsx`.
2. Grafo de dependências e recálculo incremental.
3. Estilos de célula e formatos numéricos (moeda, porcentagem, datas).
4. Mais funções, texto em fórmulas e referências absolutas (`$A$1`).
5. Núcleo em WebAssembly para a versão web.

## Licença

Ainda não definida.
