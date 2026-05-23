        while let Some(Reverse(current)) = candidates.pop() {
            if results.len() >= self.config.beam_width {
                let peeked = results.peek().unwrap(); // unwrap
                if current.distance > peeked.distance {
                    break;
                }
            }

            let node = self.load_node(current.index)?;
            for &neighbor in &node.neighbors {
                if visited.insert(neighbor) {
                    let n_node = self.load_node(neighbor)?;
                    let d = compute_distance(query, &n_node.vector, self.config.distance_metric)?;
                    let new_cand = SearchCandidate {
                        index: neighbor,
                        distance: d,
                    };

                    let mut should_add = results.len() < self.config.beam_width;
                    if !should_add {
                        let peeked = results.peek().unwrap(); // unwrap
                        if d < peeked.distance {
                            should_add = true;
                        }
                    }
                    if should_add {
                        candidates.push(Reverse(new_cand.clone()));
                        results.push(new_cand);
                        if results.len() > self.config.beam_width {
                            results.pop();
                        }
                    }
                }
            }
        }
