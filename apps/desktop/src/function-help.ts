/**
 * Ajuda das funções mais usadas: os argumentos (opcionais entre colchetes; `…`
 * marca que pode se repetir) e uma frase sobre o que a função faz.
 */
export interface FunctionHelp {
  args: string[];
  doc: string;
}

const h = (doc: string, ...args: string[]): FunctionHelp => ({ doc, args });

export const FUNCTION_HELP: Record<string, FunctionHelp> = {
  // Matemática
  SOMA: h("Soma os números dos argumentos.", "núm1", "[núm2]", "…"),
  SOMASE: h("Soma as células que atendem a um critério.", "intervalo", "critério", "[intervalo_soma]"),
  SOMASES: h("Soma as células que atendem a vários critérios.", "intervalo_soma", "intervalo_critérios1", "critérios1", "…"),
  SOMARPRODUTO: h("Multiplica as matrizes e soma os produtos.", "matriz1", "[matriz2]", "…"),
  MULT: h("Multiplica os números dos argumentos.", "núm1", "[núm2]", "…"),
  ABS: h("Valor absoluto (sem o sinal) de um número.", "núm"),
  ARRED: h("Arredonda um número para o número de casas indicado.", "núm", "núm_dígitos"),
  "ARREDONDAR.PARA.CIMA": h("Arredonda um número para cima, afastando-o de zero.", "núm", "núm_dígitos"),
  "ARREDONDAR.PARA.BAIXO": h("Arredonda um número para baixo, em direção a zero.", "núm", "núm_dígitos"),
  INT: h("Arredonda um número para baixo, até o inteiro mais próximo.", "núm"),
  TRUNCAR: h("Corta um número, deixando só as casas indicadas.", "núm", "[núm_dígitos]"),
  MOD: h("Resto da divisão de um número por outro.", "núm", "divisor"),
  POTÊNCIA: h("Eleva um número a uma potência.", "núm", "potência"),
  RAIZ: h("Raiz quadrada de um número.", "núm"),
  PI: h("O valor de pi, 3,14159…"),
  ALEATÓRIO: h("Número aleatório entre 0 e 1."),
  ALEATÓRIOENTRE: h("Inteiro aleatório entre dois valores.", "inferior", "superior"),
  MÁXIMO: h("O maior valor dos argumentos.", "núm1", "[núm2]", "…"),
  MÍNIMO: h("O menor valor dos argumentos.", "núm1", "[núm2]", "…"),
  MÁXIMOSES: h("O maior valor entre as células que atendem aos critérios.", "intervalo_máx", "intervalo_critérios1", "critérios1", "…"),
  MÍNIMOSES: h("O menor valor entre as células que atendem aos critérios.", "intervalo_mín", "intervalo_critérios1", "critérios1", "…"),
  // Estatística
  MÉDIA: h("Média aritmética dos argumentos.", "núm1", "[núm2]", "…"),
  MÉDIASE: h("Média das células que atendem a um critério.", "intervalo", "critério", "[intervalo_média]"),
  MÉDIASES: h("Média das células que atendem a vários critérios.", "intervalo_média", "intervalo_critérios1", "critérios1", "…"),
  MED: h("A mediana (valor do meio) dos números.", "núm1", "[núm2]", "…"),
  MODO: h("O valor que mais se repete.", "núm1", "[núm2]", "…"),
  "CONT.NÚM": h("Conta quantas células têm números.", "valor1", "[valor2]", "…"),
  "CONT.VALORES": h("Conta quantas células não estão vazias.", "valor1", "[valor2]", "…"),
  "CONT.SE": h("Conta as células que atendem a um critério.", "intervalo", "critério"),
  "CONT.SES": h("Conta as células que atendem a vários critérios.", "intervalo_critérios1", "critérios1", "…"),
  "CONTAR.VAZIO": h("Conta as células vazias do intervalo.", "intervalo"),
  MAIOR: h("O k-ésimo maior valor de um conjunto.", "matriz", "k"),
  MENOR: h("O k-ésimo menor valor de um conjunto.", "matriz", "k"),
  "ORDEM.EQ": h("Posição de um número numa lista ordenada.", "núm", "ref", "[ordem]"),
  // Lógica
  SE: h("Devolve um valor se a condição for verdadeira e outro se for falsa.", "teste_lógico", "valor_se_verdadeiro", "[valor_se_falso]"),
  SES: h("Testa várias condições e devolve o valor da primeira verdadeira.", "teste1", "valor_se_verdadeiro1", "…"),
  SEERRO: h("Devolve um valor alternativo se a fórmula der erro.", "valor", "valor_se_erro"),
  E: h("Verdadeiro se todos os argumentos forem verdadeiros.", "lógico1", "[lógico2]", "…"),
  OU: h("Verdadeiro se algum argumento for verdadeiro.", "lógico1", "[lógico2]", "…"),
  NÃO: h("Inverte verdadeiro e falso.", "lógico"),
  ESCOLHER: h("Escolhe um valor da lista pelo número do índice.", "núm_índice", "valor1", "[valor2]", "…"),
  PARÂMETRO: h("Compara um valor com uma lista e devolve o resultado do primeiro igual.", "expressão", "valor1", "resultado1", "…"),
  ÉERRO: h("Verdadeiro se o valor for um erro.", "valor"),
  "ÉNÚM": h("Verdadeiro se o valor for um número.", "valor"),
  "ÉTEXTO": h("Verdadeiro se o valor for texto.", "valor"),
  "ÉCÉL.VAZIA": h("Verdadeiro se a célula estiver vazia.", "valor"),
  // Procura
  PROCV: h("Procura um valor na primeira coluna e devolve outra coluna da mesma linha.", "valor_procurado", "matriz_tabela", "núm_índice_coluna", "[procurar_intervalo]"),
  PROCH: h("Procura um valor na primeira linha e devolve outra linha da mesma coluna.", "valor_procurado", "matriz_tabela", "núm_índice_linha", "[procurar_intervalo]"),
  PROCX: h("Procura num intervalo e devolve o item correspondente de outro.", "valor_procurado", "matriz_procurada", "matriz_retorno", "[se_não_encontrado]", "[modo_correspondência]", "[modo_pesquisa]"),
  CORRESP: h("Posição de um valor num intervalo.", "valor_procurado", "matriz_procurada", "[tipo_correspondência]"),
  ÍNDICE: h("Valor da célula na linha e coluna indicadas.", "matriz", "núm_linha", "[núm_coluna]"),
  DESLOC: h("Referência deslocada a partir de uma célula.", "ref", "linhas", "colunas", "[altura]", "[largura]"),
  INDIRETO: h("Referência dada por um texto.", "texto_ref"),
  LIN: h("Número da linha de uma referência.", "[ref]"),
  COL: h("Número da coluna de uma referência.", "[ref]"),
  LINS: h("Quantidade de linhas de uma referência.", "matriz"),
  COLS: h("Quantidade de colunas de uma referência.", "matriz"),
  FILTRO: h("Filtra um intervalo pelas linhas que atendem à condição.", "matriz", "incluir", "[se_vazio]"),
  CLASSIFICAR: h("Ordena as linhas de um intervalo.", "matriz", "[índice_classificação]", "[ordem]", "[por_col]"),
  ÚNICO: h("Valores únicos de um intervalo.", "matriz", "[por_col]", "[exatamente_uma_vez]"),
  SEQUÊNCIA: h("Gera uma sequência de números.", "linhas", "[colunas]", "[início]", "[passo]"),
  // Texto
  CONCATENAR: h("Junta vários textos em um.", "texto1", "[texto2]", "…"),
  CONCAT: h("Junta os textos de vários intervalos.", "texto1", "[texto2]", "…"),
  UNIRTEXTO: h("Junta textos com um separador.", "delimitador", "ignorar_vazio", "texto1", "[texto2]", "…"),
  ESQUERDA: h("Os primeiros caracteres de um texto.", "texto", "[núm_caract]"),
  DIREITA: h("Os últimos caracteres de um texto.", "texto", "[núm_caract]"),
  "EXT.TEXTO": h("Caracteres do meio de um texto.", "texto", "núm_inicial", "núm_caract"),
  "NÚM.CARACT": h("Número de caracteres de um texto.", "texto"),
  MAIÚSCULA: h("Converte o texto para maiúsculas.", "texto"),
  MINÚSCULA: h("Converte o texto para minúsculas.", "texto"),
  "PRI.MAIÚSCULA": h("Põe a primeira letra de cada palavra em maiúscula.", "texto"),
  ARRUMAR: h("Tira os espaços extras de um texto.", "texto"),
  TEXTO: h("Converte um valor em texto com o formato indicado.", "valor", "formato_texto"),
  VALOR: h("Converte um texto em número.", "texto"),
  SUBSTITUIR: h("Troca um trecho do texto por outro.", "texto", "texto_antigo", "novo_texto", "[núm_ocorrência]"),
  PROCURAR: h("Posição de um texto dentro de outro (diferencia maiúsculas).", "texto_procurado", "no_texto", "[núm_inicial]"),
  LOCALIZAR: h("Posição de um texto dentro de outro (não diferencia maiúsculas).", "texto_procurado", "no_texto", "[núm_inicial]"),
  REPT: h("Repete um texto o número de vezes indicado.", "texto", "núm_vezes"),
  // Data e hora
  HOJE: h("A data de hoje."),
  AGORA: h("A data e a hora de agora."),
  DATA: h("Monta uma data a partir de ano, mês e dia.", "ano", "mês", "dia"),
  ANO: h("O ano de uma data.", "núm_série"),
  MÊS: h("O mês de uma data.", "núm_série"),
  DIA: h("O dia do mês de uma data.", "núm_série"),
  "DIA.DA.SEMANA": h("O dia da semana de uma data (1 a 7).", "núm_série", "[retornar_tipo]"),
  DATAM: h("A data que fica alguns meses antes ou depois.", "data_inicial", "meses"),
  FIMMÊS: h("O último dia do mês, alguns meses antes ou depois.", "data_inicial", "meses"),
  DATADIF: h("Diferença entre duas datas em dias, meses ou anos.", "data_inicial", "data_final", "unidade"),
  DIATRABALHO: h("A data após alguns dias úteis.", "data_inicial", "dias", "[feriados]"),
  "DIATRABALHOTOTAL": h("Dias úteis entre duas datas.", "data_inicial", "data_final", "[feriados]"),
  // Financeiras
  PGTO: h("Valor da parcela de um empréstimo.", "taxa", "nper", "vp", "[vf]", "[tipo]"),
  VP: h("Valor presente de um investimento.", "taxa", "nper", "pgto", "[vf]", "[tipo]"),
  VF: h("Valor futuro de um investimento.", "taxa", "nper", "pgto", "[vp]", "[tipo]"),
  NPER: h("Número de períodos de um investimento.", "taxa", "pgto", "vp", "[vf]", "[tipo]"),
  TAXA: h("Taxa de juros por período.", "nper", "pgto", "vp", "[vf]", "[tipo]"),
  VPL: h("Valor presente líquido de uma série de fluxos.", "taxa", "valor1", "[valor2]", "…"),
  TIR: h("Taxa interna de retorno de uma série de fluxos.", "valores", "[estimativa]"),
};

/**
 * A função cujos parênteses estão abertos na posição do cursor, e em qual
 * argumento ele está (a partir de zero). `null` fora de qualquer chamada.
 * Textos entre aspas não contam.
 */
export function callAt(text: string, caret: number): { name: string; arg: number } | null {
  if (!text.startsWith("=")) return null;
  const stack: { name: string; arg: number }[] = [];
  let quoted = false;
  for (let i = 1; i < caret; i++) {
    const c = text[i];
    if (c === '"') quoted = !quoted;
    if (quoted) continue;
    if (c === "(") {
      const before = /([\p{L}][\p{L}\p{N}._]*)$/u.exec(text.slice(0, i));
      stack.push({ name: before ? before[1] : "", arg: 0 });
    } else if (c === ")") stack.pop();
    else if (c === ";" || c === ",") {
      const top = stack[stack.length - 1];
      if (top) top.arg++;
    }
  }
  // Parênteses soltos (de agrupamento, sem função) não têm ajuda.
  for (let i = stack.length - 1; i >= 0; i--) if (stack[i].name) return stack[i];
  return null;
}

/** Qual argumento da assinatura destacar: os que se repetem (`…`) valem para os seguintes. */
export function argIndex(args: string[], arg: number): number {
  if (arg < args.length) return args[arg] === "…" ? Math.max(0, args.length - 2) : arg;
  return args[args.length - 1] === "…" ? Math.max(0, args.length - 2) : -1;
}
