use std::collections::{HashMap, HashSet};

/// Histórico de ~4s de níveis de áudio (a 100ms por amostra).
pub const AMOSTRAS_DE_ECO: usize = 40;
/// Atrasos testados na correlação: de 1 a 4 amostras (100ms a 400ms).
pub const ATRASOS_TESTADOS: [usize; 4] = [1, 2, 3, 4];
/// Correlação de Pearson mínima com atraso para considerar suspeito.
pub const CORRELACAO_MINIMA: f32 = 0.72;
/// Número de ciclos seguidos com suspeita para disparar o aviso.
pub const CICLOS_PRA_AVISAR: usize = 5;

/// Detector de eco / microfonia acústica entre o som emitido e retornado.
///
/// Quem detecta é sempre o participante que emitiu o som: ele compara seu
/// próprio nível de fala gravado com o nível que recebe de cada par remoto.
/// Quando o áudio do par acompanha o seu próprio com um atraso de 100–400ms
/// e com correlação superior ao atraso zero (simultâneo), é sinal de vazamento
/// por caixa de som de volta ao microfone do interlocutor.
#[derive(Debug)]
pub struct DetectorEco {
    historico_meu: Vec<f32>,
    historico_peers: HashMap<String, Vec<f32>>,
    suspeitas: HashMap<String, usize>,
    ja_avisados: HashSet<String>,
}

impl Default for DetectorEco {
    fn default() -> Self {
        Self::new()
    }
}

impl DetectorEco {
    pub fn new() -> Self {
        Self {
            historico_meu: Vec::with_capacity(AMOSTRAS_DE_ECO),
            historico_peers: HashMap::new(),
            suspeitas: HashMap::new(),
            ja_avisados: HashSet::new(),
        }
    }

    /// Registra uma amostra de nível (0.0 a 100.0) do meu microfone local.
    pub fn registra_meu_nivel(&mut self, nivel: f32) {
        self.historico_meu.push(nivel);
        if self.historico_meu.len() > AMOSTRAS_DE_ECO {
            self.historico_meu.remove(0);
        }
    }

    /// Registra uma amostra de nível (0.0 a 100.0) recebida de um par remoto.
    pub fn registra_nivel_peer(&mut self, id: &str, nivel: f32) {
        let hist = self.historico_peers.entry(id.to_string()).or_default();
        hist.push(nivel);
        if hist.len() > AMOSTRAS_DE_ECO {
            hist.remove(0);
        }
    }

    /// Remove dados e histórico de um par que saiu da chamada.
    pub fn esquece_peer(&mut self, id: &str) {
        self.historico_peers.remove(id);
        self.suspeitas.remove(id);
        self.ja_avisados.remove(id);
    }

    /// Limpa todos os estados e históricos (ex.: ao sair da chamada ou reconectar).
    pub fn limpar(&mut self) {
        self.historico_meu.clear();
        self.historico_peers.clear();
        self.suspeitas.clear();
        self.ja_avisados.clear();
    }

    /// Avalia correlações e retorna uma lista de IDs para os quais enviar `echo-warning`.
    pub fn verificar_eco(&mut self) -> Vec<String> {
        let mut para_avisar = Vec::new();
        if self.historico_meu.len() < AMOSTRAS_DE_ECO {
            return para_avisar;
        }

        // Sem eu falando, não há eco do meu som a ser procurado.
        let meu_pico = self.historico_meu.iter().copied().fold(0.0f32, f32::max);
        if meu_pico < 20.0 {
            for v in self.suspeitas.values_mut() {
                *v = 0;
            }
            return para_avisar;
        }

        for (id, hist_peer) in &self.historico_peers {
            if self.ja_avisados.contains(id) {
                continue;
            }
            if hist_peer.len() < AMOSTRAS_DE_ECO {
                continue;
            }

            // O melhor casamento COM ATRASO tem que superar o casamento simultâneo:
            // duas pessoas falando ao mesmo tempo batem no atraso zero, eco não.
            let simultaneo = correlacao(&self.historico_meu, hist_peer);
            let mut melhor_atrasado = 0.0f32;

            for &atraso in &ATRASOS_TESTADOS {
                if atraso >= self.historico_meu.len() || atraso >= hist_peer.len() {
                    continue;
                }
                let meu_cortado = &self.historico_meu[..self.historico_meu.len() - atraso];
                let peer_cortado = &hist_peer[atraso..];
                let c = correlacao(meu_cortado, peer_cortado);
                if c > melhor_atrasado {
                    melhor_atrasado = c;
                }
            }

            let parece_eco = melhor_atrasado > CORRELACAO_MINIMA && melhor_atrasado > simultaneo;
            let suspeita = self.suspeitas.entry(id.clone()).or_insert(0);
            if parece_eco {
                *suspeita += 1;
            } else {
                *suspeita = 0;
            }

            if *suspeita >= CICLOS_PRA_AVISAR {
                self.ja_avisados.insert(id.clone());
                *suspeita = 0;
                para_avisar.push(id.clone());
            }
        }

        para_avisar
    }
}

/// Coeficiente de correlação linear de Pearson entre duas séries temporais.
pub fn correlacao(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n < 10 {
        return 0.0;
    }
    let soma_a: f32 = a[..n].iter().sum();
    let soma_b: f32 = b[..n].iter().sum();
    let media_a = soma_a / (n as f32);
    let media_b = soma_b / (n as f32);

    let mut num = 0.0f32;
    let mut den_a = 0.0f32;
    let mut den_b = 0.0f32;

    for i in 0..n {
        let da = a[i] - media_a;
        let db = b[i] - media_b;
        num += da * db;
        den_a += da * da;
        den_b += db * db;
    }

    if den_a <= 0.0 || den_b <= 0.0 {
        return 0.0;
    }

    num / (den_a * den_b).sqrt()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn correlacao_identica_e_inversa() {
        let v1: Vec<f32> = (0..20).map(|i| i as f32).collect();
        let v2: Vec<f32> = v1.clone();
        let r_id = correlacao(&v1, &v2);
        assert!((r_id - 1.0).abs() < 1e-4);

        let v3: Vec<f32> = (0..20).map(|i| (20 - i) as f32).collect();
        let r_inv = correlacao(&v1, &v3);
        assert!((r_inv - (-1.0)).abs() < 1e-4);
    }

    #[test]
    fn correlacao_vetores_curtos_ou_constantes() {
        let v1 = vec![1.0; 5];
        let v2 = vec![2.0; 5];
        assert_eq!(correlacao(&v1, &v2), 0.0);

        let v_const = vec![5.0; 20];
        let v_alt: Vec<f32> = (0..20).map(|i| i as f32).collect();
        assert_eq!(correlacao(&v_const, &v_alt), 0.0);
    }

    #[test]
    fn detecta_eco_com_atraso_exatamente_uma_vez() {
        let mut detector = DetectorEco::new();
        let onda = |i: f32| 40.0 + 35.0 * (i / 3.0).sin();

        let mut avisos_totais = 0;
        for ciclo in 0..60 {
            detector.registra_meu_nivel(onda(ciclo as f32));
            detector.registra_nivel_peer("ana", onda((ciclo - 2) as f32));
            let avisos = detector.verificar_eco();
            if avisos.contains(&"ana".to_string()) {
                avisos_totais += 1;
            }
        }

        assert_eq!(
            avisos_totais, 1,
            "eco com atraso deve disparar exatamente um aviso"
        );
    }

    #[test]
    fn fala_simultanea_sem_atraso_nao_gera_aviso() {
        let mut detector = DetectorEco::new();
        let onda = |i: f32| 40.0 + 35.0 * (i / 3.0).sin();

        let mut avisos_totais = 0;
        for ciclo in 0..60 {
            let valor = onda(ciclo as f32);
            detector.registra_meu_nivel(valor);
            detector.registra_nivel_peer("ana", valor);
            let avisos = detector.verificar_eco();
            if avisos.contains(&"ana".to_string()) {
                avisos_totais += 1;
            }
        }

        assert_eq!(
            avisos_totais, 0,
            "fala simultânea em atraso zero não pode gerar falso positivo de eco"
        );
    }

    #[test]
    fn silencio_proprio_nao_dispara_aviso() {
        let mut detector = DetectorEco::new();
        let onda = |i: f32| 40.0 + 35.0 * (i / 3.0).sin();

        for ciclo in 0..60 {
            detector.registra_meu_nivel(5.0); // abaixo de pico 20.0
            detector.registra_nivel_peer("ana", onda(ciclo as f32));
            let avisos = detector.verificar_eco();
            assert!(avisos.is_empty());
        }
    }

    #[test]
    fn esquece_peer_reseta_estado() {
        let mut detector = DetectorEco::new();
        detector.registra_nivel_peer("ana", 50.0);
        assert!(detector.historico_peers.contains_key("ana"));
        detector.esquece_peer("ana");
        assert!(!detector.historico_peers.contains_key("ana"));
    }
}
