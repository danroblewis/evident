(goal
  (let ((a!1 (re.++ (re.++ (re.* (re.range "a" "z")) (str.to_re "ab"))
                    (re.* (re.range "a" "z")))))
  (let ((a!2 (re.inter a!1 (re.++ (re.* (re.range "a" "z")) (str.to_re "z")))))
    (str.in_re st a!2)))
  (= (str.len st) 12))