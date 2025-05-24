(ns cliche.main (:require [babashka.cli :as cli]))

(def cli-options {:port {:default 80 :coerce :long} :help {:coerce :boolean}})

(defn -main [& _args]
  (prn (cli/parse-opts *command-line-args* cli-options)))
